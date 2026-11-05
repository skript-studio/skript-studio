//! Adversarial UTF-16/byte boundary suite (T02 / findings C1 + H2).
//!
//! LSP positions are UTF-16 code units, while the engines index UTF-8 bytes.
//! These tests drive the real `Backend` handlers in-process with documents
//! containing section-sign colour codes, CJK, Cyrillic, emoji surrogate
//! pairs and combining marks. They assert three invariants:
//!
//! * no request panics (the pre-fix selectionRange handler aborted the
//!   process on a line of U+00A7 characters);
//! * every produced position stays inside its line's true UTF-16 length
//!   (the pre-fix code leaked byte columns);
//! * hovering the ASCII tail after colour codes highlights that word.

use skript_lsp::backend::Backend;
use skript_lsp::options::EffectiveConfig;
use skript_lsp::state::AppState;
use tower_lsp::lsp_types::{
    CodeActionContext, CodeActionParams, CompletionParams, Diagnostic, DiagnosticSeverity,
    DocumentHighlightParams, DocumentSymbolParams, FoldingRangeParams, GotoDefinitionParams,
    HoverParams, InlayHintParams, NumberOrString, Position, Range, ReferenceContext,
    ReferenceParams, RenameParams, SelectionRange, SelectionRangeParams, SemanticTokensParams,
    SemanticTokensResult, SignatureHelpParams, TextDocumentIdentifier, TextDocumentPositionParams,
    Url, WorkDoneProgressParams,
};
use tower_lsp::LanguageServer;

const URI: &str = "file:///position-boundaries.sk";

struct Harness {
    service: tower_lsp::LspService<Backend>,
    // The transport socket must stay alive for the client channel.
    _socket: tower_lsp::ClientSocket,
    rt: tokio::runtime::Runtime,
    state: AppState,
    uri: Url,
}

impl Harness {
    fn new(text: &str) -> Self {
        let state = AppState::new(EffectiveConfig::default());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        // Populate the EntryTable the same way initialize() does, then build
        // the pattern engine so hover resolves through the real corpus.
        rt.block_on(state.refresh());
        assert!(state.ready(), "entry table must be installed");
        state.ensure_engine();
        assert!(state.engine_stats().is_some(), "engine must be ready");
        state.open_doc(URI.to_owned(), text.to_owned());

        let (service, socket) =
            tower_lsp::LspService::new(|client| Backend::new(client, state.clone()));
        Harness {
            service,
            _socket: socket,
            rt,
            state,
            uri: Url::parse(URI).expect("valid test URI"),
        }
    }

    fn backend(&self) -> &Backend {
        self.service.inner()
    }

    fn document(&self) -> TextDocumentIdentifier {
        TextDocumentIdentifier {
            uri: self.uri.clone(),
        }
    }

    fn position(&self, line: u32, character: u32) -> TextDocumentPositionParams {
        TextDocumentPositionParams {
            text_document: self.document(),
            position: Position::new(line, character),
        }
    }

    /// Swap the document text without publishing diagnostics. T03 owns the
    /// engine-thread panic that `didOpen` on a U+00A7 line still triggers;
    /// this suite isolates the position boundary.
    fn open(&self, text: String) {
        self.state.open_doc(URI.to_owned(), text);
    }
}

/// Split into LSP lines; `split('\n')` keeps a trailing empty line, unlike
/// `str::lines`, which matters for end-of-document positions.
fn lines(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

fn utf16_len(line: &str) -> u32 {
    line.encode_utf16().count() as u32
}

fn assert_position_in_document(pos: Position, text: &str) {
    let lines = lines(text);
    let line = lines
        .get(pos.line as usize)
        .unwrap_or_else(|| panic!("line {} is outside the document", pos.line));
    assert!(
        pos.character <= utf16_len(line),
        "position {pos:?} exceeds UTF-16 line length {} of {:?}",
        utf16_len(line),
        line
    );
}

fn assert_range_in_document(range: Range, text: &str) {
    assert_position_in_document(range.start, text);
    assert_position_in_document(range.end, text);
    assert!(
        range.start.line < range.end.line
            || (range.start.line == range.end.line && range.start.character <= range.end.character),
        "inverted range {range:?}"
    );
}

fn assert_selection_chain(selection: &SelectionRange, text: &str) {
    assert_range_in_document(selection.range, text);
    if let Some(parent) = &selection.parent {
        assert_selection_chain(parent, text);
    }
}

/// Every line shape named by the work order, plus a mixed tab/colour line.
const DOCUMENTS: &[&str] = &[
    // The exact R-C1 repro: four U+00A7 on line 0, character 1.
    "§§§§",
    // The exact R-H2 line at top level as well as inside an event body.
    "broadcast §a§b§c§dhello world",
    "on load:\n\t§§§§\n",
    "on load:\n\tbroadcast §a§b§c§dhello world\n",
    "on load:\n\tbroadcast \"广播 你好\"\n",
    "on load:\n\tbroadcast \"привет мир\"\n",
    "on load:\n\tbroadcast \"😀🎉 emoji\"\n",
    "function test():\n\tbroadcast \"混合 §a\t😀\"\n",
];

#[test]
fn selection_range_survives_every_utf16_column() {
    let h = Harness::new(DOCUMENTS[0]);

    for document in DOCUMENTS {
        h.open((*document).to_owned());
        let positions = lines(document)
            .iter()
            .enumerate()
            .flat_map(|(line, text)| {
                let line = line as u32;
                let mut columns: Vec<u32> = (0..=utf16_len(text)).collect();
                // One past the line end and well past it must clamp.
                columns.push(utf16_len(text) + 1);
                columns.push(utf16_len(text) + 100);
                columns
                    .into_iter()
                    .map(move |character| Position::new(line, character))
            })
            .collect();

        // The pre-fix handler panicked here on the four-U+00A7 line.
        let result =
            h.rt.block_on(h.backend().selection_range(SelectionRangeParams {
                text_document: h.document(),
                positions,
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: Default::default(),
            }))
            .expect("selectionRange must answer")
            .expect("selectionRange must return ranges");
        assert!(!result.is_empty());
        for selection in &result {
            assert_selection_chain(selection, document);
        }
    }
}

#[test]
fn line_and_document_ranges_end_at_the_utf16_line_length() {
    let text = "broadcast §a§b§c§dhello world";
    let h = Harness::new(text);
    let result =
        h.rt.block_on(h.backend().selection_range(SelectionRangeParams {
            text_document: h.document(),
            positions: vec![Position::new(0, 1)],
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        }))
        .expect("selectionRange must answer")
        .expect("selectionRange must return a range");

    let word = &result[0];
    let line = word.parent.as_ref().expect("line selection");
    let document = line.parent.as_ref().expect("document selection");
    let expected = Position::new(0, text.encode_utf16().count() as u32);
    assert_eq!(line.range.end, expected);
    assert_eq!(document.range.end, expected);
    assert_ne!(
        line.range.end.character as usize,
        text.len(),
        "the byte length must never be reported as an LSP column"
    );
}

#[test]
fn hover_highlights_the_utf16_word_after_colour_codes() {
    let text = "on load:\n\tbroadcast §a§b§c§dhello world\n";
    let h = Harness::new(text);

    // Line 1 is `\tbroadcast §a§b§c§dhello world`:
    // tab 0, "broadcast" 1..=9, space 10, colour codes 11..=18,
    // "hello" 19..=23, space 24, "world" 25..=29.
    let hovers =
        h.rt.block_on(h.backend().hover(HoverParams {
            text_document_position_params: h.position(1, 19),
            work_done_progress_params: WorkDoneProgressParams::default(),
        }))
        .expect("hover must answer")
        .expect("hover must resolve 'hello' through the engine corpus");

    let range = hovers.range.expect("hover range");
    assert_range_in_document(range, text);
    assert_eq!(range.start, Position::new(1, 19), "byte column leaked");
    assert_eq!(
        range.end,
        Position::new(1, 24),
        "hover must cover exactly 'hello' in UTF-16 columns"
    );

    // The process must stay alive for follow-up requests (pre-fix C1 exited
    // with code 101 after the selectionRange panic).
    h.rt.block_on(h.backend().completion(CompletionParams {
        text_document_position: h.position(1, 19),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: Default::default(),
        context: None,
    }))
    .expect("follow-up completion must answer");
}

#[test]
fn every_position_consuming_provider_accepts_non_ascii() {
    let text = "function greet(name: text, age: number):\n\
                \tbroadcast \"§a%{name}% 广播 😀\"\n\
                \tset {_x} to name\n\
                \tbroadcast {_x}\n\
                on load:\n\
                \tgreet(\"hi\", 3)\n";
    let h = Harness::new(text);

    let ctx = h.position(2, 7); // `x` in `set {_x} to name`
    h.rt.block_on(h.backend().completion(CompletionParams {
        text_document_position: h.position(1, 12),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: Default::default(),
        context: None,
    }))
    .expect("completion must answer");

    h.rt.block_on(h.backend().hover(HoverParams {
        text_document_position_params: h.position(1, 12),
        work_done_progress_params: WorkDoneProgressParams::default(),
    }))
    .expect("hover must answer");

    h.rt.block_on(h.backend().signature_help(SignatureHelpParams {
        text_document_position_params: h.position(5, 13),
        work_done_progress_params: WorkDoneProgressParams::default(),
        context: None,
    }))
    .expect("signature help must answer");

    let definition =
        h.rt.block_on(h.backend().goto_definition(GotoDefinitionParams {
            text_document_position_params: ctx.clone(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        }))
        .expect("definition must answer");
    if let Some(tower_lsp::lsp_types::GotoDefinitionResponse::Scalar(location)) = definition {
        assert_range_in_document(location.range, text);
    }

    let references =
        h.rt.block_on(h.backend().references(ReferenceParams {
            text_document_position: ctx.clone(),
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        }))
        .expect("references must answer");
    if let Some(refs) = references {
        for location in refs {
            assert_range_in_document(location.range, text);
        }
    }

    let highlights =
        h.rt.block_on(h.backend().document_highlight(DocumentHighlightParams {
            text_document_position_params: ctx.clone(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        }))
        .expect("highlights must answer");
    if let Some(highlights) = highlights {
        for highlight in highlights {
            assert_range_in_document(highlight.range, text);
        }
    }

    let rename =
        h.rt.block_on(h.backend().rename(RenameParams {
            text_document_position: ctx,
            new_name: "y".to_owned(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        }))
        .expect("rename must answer");
    if let Some(edit) = rename {
        if let Some(changes) = edit.changes {
            for edits in changes.into_values() {
                for text_edit in edits {
                    assert_range_in_document(text_edit.range, text);
                }
            }
        }
    }

    // A quick fix on a line with non-ASCII text must report the line's
    // UTF-16 length, not its byte length.
    let unused_line = "\tset {_x} to name";
    let actions =
        h.rt.block_on(h.backend().code_action(CodeActionParams {
            text_document: h.document(),
            range: Range::new(Position::new(2, 0), Position::new(2, 0)),
            context: CodeActionContext {
                diagnostics: vec![Diagnostic {
                    range: Range::new(Position::new(2, 0), Position::new(2, 1)),
                    severity: Some(DiagnosticSeverity::HINT),
                    code: Some(NumberOrString::String("unused-variable".to_owned())),
                    source: Some("skript".to_owned()),
                    message: "Unused local variable: _x".to_owned(),
                    ..Default::default()
                }],
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        }))
        .expect("code action must answer")
        .expect("code action must return a response");
    assert_eq!(actions.len(), 1);
    let tower_lsp::lsp_types::CodeActionOrCommand::CodeAction(action) = &actions[0] else {
        panic!("expected a code action");
    };
    let edit = action
        .edit
        .as_ref()
        .and_then(|e| e.changes.as_ref())
        .unwrap();
    let edits = edit.values().next().unwrap();
    assert_eq!(
        edits[0].range.end,
        Position::new(2, utf16_len(unused_line)),
        "quick-fix range must use UTF-16, not bytes"
    );

    h.rt.block_on(h.backend().document_symbol(DocumentSymbolParams {
        text_document: h.document(),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: Default::default(),
    }))
    .expect("document symbols must answer");

    h.rt.block_on(h.backend().folding_range(FoldingRangeParams {
        text_document: h.document(),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: Default::default(),
    }))
    .expect("folding ranges must answer");

    let hints =
        h.rt.block_on(h.backend().inlay_hint(InlayHintParams {
            text_document: h.document(),
            range: Range::new(Position::new(0, 0), Position::new(6, 0)),
            work_done_progress_params: WorkDoneProgressParams::default(),
        }))
        .expect("inlay hints must answer");
    if let Some(hints) = hints {
        for hint in hints {
            assert_position_in_document(hint.position, text);
        }
    }
}

#[test]
fn semantic_tokens_stay_inside_utf16_lines() {
    let text = "on load:\n\
                \tbroadcast §a§b§c§dhello 广播 😀\n\
                \tset {_x} to \"привет\" # коммент\n";
    let h = Harness::new(text);

    let result =
        h.rt.block_on(h.backend().semantic_tokens_full(SemanticTokensParams {
            text_document: h.document(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: Default::default(),
        }))
        .expect("semantic tokens must answer")
        .expect("semantic tokens must return data");
    let SemanticTokensResult::Tokens(tokens) = result else {
        panic!("expected a full-token response");
    };

    let lines = lines(text);
    let mut line = 0u32;
    let mut character = 0u32;
    for token in tokens.data {
        line += token.delta_line;
        character = if token.delta_line == 0 {
            character + token.delta_start
        } else {
            token.delta_start
        };
        let line_text = lines
            .get(line as usize)
            .unwrap_or_else(|| panic!("token line {line} outside document"));
        assert!(
            character + token.length <= utf16_len(line_text),
            "token {character}..{} exceeds UTF-16 line length {} of {:?}",
            character + token.length,
            utf16_len(line_text),
            line_text
        );
    }
}
