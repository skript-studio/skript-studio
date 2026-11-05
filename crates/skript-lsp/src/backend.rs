//! LSP `LanguageServer` implementation.
//!
//! Wires `tower-lsp` to `skript-core`. Every request handler degrades
//! gracefully when [`AppState::ready`] is false: completions return empty,
//! hover returns `None`, and diagnostics are deferred until data is loaded
//! (and then re-published for every open document).
//!
//! LSP version: 3.17. We advertise only the capabilities we actually
//! implement (completion, hover, diagnostics push, semanticTokens/full,
//! document symbols, folding, go-to-definition, references, signature help,
//! inlay hints).
//!
//! # Diagnostics
//!
//! Diagnostics merge two complementary streams for maximum Skript parity.
//! **Both always run** — there is no engine selection; the two streams are
//! unconditional and their results are concatenated:
//!
//! - **AST validator** (skript-core): `parse-error`, `unused-variable`,
//!   `semantic` errors.
//! - **Pattern engine line checker** (skript-checks over skript-registry +
//!   skript-patterns, on a dedicated thread): `unparseable`,
//!   `addon-not-enabled`, `plugin-missing`, `ambiguous-addon`.
//!
//! The two streams produce different diagnostic codes and never duplicate.
//! If the pattern engine is temporarily unavailable (still building or
//! corpus load failure), only AST diagnostics are published until it
//! becomes ready, then all open documents are re-validated.

use crate::folding;
use crate::options::{EffectiveConfig, InitOptions, ParserDiagnostics, SkriptRcToml};
use crate::position::{byte_offset_to_utf16_col, utf16_col_to_byte_offset};
use crate::state::AppState;
use serde_json::Value;
use skript_core::ast::Visitor;
use skript_core::completion::{self, CompletionContext};
use skript_core::hover;
use skript_core::references;
use skript_core::tokens::{self, SemanticType};
use skript_core::validation::{self, Severity};
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::SelectionRangeProviderCapability;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, CompletionItem, CompletionItemKind,
    CompletionResponse, Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentSymbol, DocumentSymbolParams,
    DocumentSymbolResponse, FoldingRange, FoldingRangeParams, FoldingRangeProviderCapability,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents, HoverProviderCapability,
    InitializeParams, InitializeResult, InitializedParams, InlayHint, InlayHintKind,
    InlayHintLabel, InlayHintParams, InsertTextFormat, Location, MarkupContent, MarkupKind, OneOf,
    ParameterInformation, ParameterLabel, Position, Range, ReferenceParams, SemanticToken,
    SemanticTokenType, SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensResult, SemanticTokensServerCapabilities,
    ServerCapabilities, SignatureHelp, SignatureHelpOptions, SignatureHelpParams,
    SignatureInformation, SymbolKind, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit,
    WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer};

/// Semantic-token legend advertised in server capabilities. Order matters:
/// the integer token type ids returned in `SemanticTokens` index into this.
const TOKEN_LEGEND: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,
    SemanticTokenType::TYPE,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::STRING,
    SemanticTokenType::COMMENT,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::EVENT,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::NUMBER,
];

/// Map a `skript_core::tokens::SemanticType` to its index in `TOKEN_LEGEND`.
fn token_type_index(ty: SemanticType) -> u32 {
    match ty {
        SemanticType::Keyword => 0,
        SemanticType::Type => 1,
        SemanticType::Variable => 2,
        SemanticType::String => 3,
        SemanticType::Comment => 4,
        SemanticType::Operator => 5,
        SemanticType::Event => 6,
        SemanticType::Function => 7,
        SemanticType::Number => 8,
    }
}

/// Per-session server instance. Cheap to clone (state is `Arc`-backed).
pub struct Backend {
    client: Client,
    state: AppState,
}

impl Backend {
    pub fn new(client: Client, state: AppState) -> Self {
        Backend { client, state }
    }

    /// Apply `initializationOptions` on top of the existing effective config.
    fn apply_init_options(&self, params: &InitializeParams) {
        let Some(Value::Object(map)) = params.initialization_options.as_ref() else {
            return;
        };
        let value = Value::Object(map.clone());
        let Ok(opts) = serde_json::from_value::<InitOptions>(value) else {
            tracing::warn!("initializationOptions present but failed to parse; ignoring");
            return;
        };
        let mut cfg = self.state.config();
        if let Some(max) = opts.max_completions {
            cfg.max_completions = max;
        }
        if let Some(addons) = opts.enabled_addons {
            cfg.enabled_addons = addons;
        }
        if let Some(plugins) = opts.installed_plugins {
            cfg.installed_plugins = plugins;
        }
        if let Some(sv) = opts.skript_version {
            cfg.skript_version = Some(sv);
        }
        if let Some(mv) = opts.minecraft_version {
            cfg.minecraft_version = Some(mv);
        }
        if let Some(pd) = opts
            .parser_diagnostics
            .as_deref()
            .and_then(ParserDiagnostics::parse)
        {
            cfg.parser_diagnostics = pd;
        }
        self.state.set_config(cfg);
    }

    /// Re-validate every open document and publish fresh diagnostics.
    /// Called after the index becomes ready and on every text change.
    async fn publish_diagnostics_for_all(&self) {
        if !self.state.ready() {
            return;
        }
        // Collect URIs first to avoid holding a lock across the await.
        let uris: Vec<String> = self.state.doc_uris();
        for uri in uris {
            self.publish_diagnostics(&uri).await;
        }
    }

    async fn publish_diagnostics(&self, uri: &str) {
        publish_merged_diagnostics(&self.state, &self.client, uri).await;
    }
}

fn severity_to_lsp(s: Severity) -> DiagnosticSeverity {
    match s {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Information => DiagnosticSeverity::INFORMATION,
        Severity::Hint => DiagnosticSeverity::HINT,
    }
}

/// skript-core AST validation mapped to LSP diagnostics.
///
/// Produces: parse-error, unused-variable, semantic. These are the
/// language-level checks that don't depend on the pattern engine.
///
/// Severity policy for `parse-error` follows the effective config
/// (default: demoted to Hint — the AST parser is deliberately partial
/// while the corpus line-checker is the validity authority).
fn core_lsp_diagnostics(state: &AppState, uri: &str, text: &str) -> Vec<Diagnostic> {
    let policy = state.config().parser_diagnostics;
    // AST-based validation from cached snapshot
    let diags = state
        .with_snapshot(uri, |snap| {
            validation::validate_analysis(&snap.parse_result, &snap.analysis, text)
        })
        .unwrap_or_else(|| validation::validate_program(text));

    diags
        .into_iter()
        .filter_map(|d| {
            let severity = if d.code == "parse-error" {
                match policy {
                    ParserDiagnostics::Off => return None,
                    ParserDiagnostics::Hint => DiagnosticSeverity::HINT,
                    ParserDiagnostics::Error => DiagnosticSeverity::ERROR,
                }
            } else {
                severity_to_lsp(d.severity)
            };
            // `offset_to_linecol` reports UTF-8 byte columns; the LSP
            // client expects UTF-16 code units.
            let line_text = line_text_at(text, d.line).unwrap_or("");
            let start = byte_offset_to_utf16_col(line_text, d.start_col as usize);
            let end = byte_offset_to_utf16_col(line_text, d.end_col as usize).max(start);
            Some(Diagnostic {
                range: Range::new(Position::new(d.line, start), Position::new(d.line, end)),
                severity: Some(severity),
                source: Some(d.source),
                code: Some(tower_lsp::lsp_types::NumberOrString::String(d.code)),
                message: d.message,
                ..Default::default()
            })
        })
        .collect()
}

/// Publish merged diagnostics (AST + pattern-engine line-based) for one
/// document.
///
/// Free function so it can be called from both the `Backend` method and the
/// spawned engine-build task (which doesn't own a `Backend`).
async fn publish_merged_diagnostics(state: &AppState, client: &tower_lsp::Client, uri: &str) {
    let Some(text) = state.get_doc(uri) else {
        return;
    };
    let config = state.config();

    let mut lsp_diags = core_lsp_diagnostics(state, uri, &text);
    if let Some(line_diags) = state
        .check_document_via_engine(text.clone(), build_check_config(&config))
        .await
    {
        lsp_diags.extend(
            line_diags
                .into_iter()
                .map(|d| line_diagnostic_to_lsp(d, &text)),
        );
    }

    if let Ok(parsed) = uri.parse::<tower_lsp::lsp_types::Url>() {
        client.publish_diagnostics(parsed, lsp_diags, None).await;
    }
}

/// Build the `skript-checks` configuration from the effective config.
pub fn build_check_config(config: &EffectiveConfig) -> skript_checks::CheckConfig {
    skript_checks::CheckConfig {
        enabled_addons: config.enabled_addons.iter().cloned().collect(),
        installed_plugins: config.installed_plugins.iter().cloned().collect(),
        skript_version: config.skript_version.clone(),
        minecraft_version: config.minecraft_version.clone(),
    }
}

/// Map a `skript-checks` `LineDiagnostic` to an LSP `Diagnostic`.
///
/// Severity: Error→1, Warning→2, Info→3. `LineDiagnostic` line/column
/// fields are 0-based **character** counts (Unicode scalar values), LSP
/// positions are UTF-16 code units, and the source itself is indexed in
/// UTF-8 bytes. `source` is the document text used to resolve the line so
/// both conversions stay boundary-safe.
pub fn line_diagnostic_to_lsp(d: skript_checks::LineDiagnostic, source: &str) -> Diagnostic {
    let line_text = line_text_at(source, d.line as u32).unwrap_or("");
    let start_byte = char_col_to_byte_offset(line_text, d.col_start);
    let end_byte = char_col_to_byte_offset(line_text, d.col_end);
    let start = byte_offset_to_utf16_col(line_text, start_byte);
    let end = byte_offset_to_utf16_col(line_text, end_byte).max(start);
    Diagnostic {
        range: Range::new(
            Position::new(d.line as u32, start),
            Position::new(d.line as u32, end),
        ),
        severity: Some(match d.severity {
            skript_checks::Severity::Error => DiagnosticSeverity::ERROR,
            skript_checks::Severity::Warning => DiagnosticSeverity::WARNING,
            skript_checks::Severity::Info => DiagnosticSeverity::INFORMATION,
        }),
        source: Some("skript".to_owned()),
        code: Some(tower_lsp::lsp_types::NumberOrString::String(
            d.code.to_owned(),
        )),
        message: d.message,
        ..Default::default()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> RpcResult<InitializeResult> {
        // 1. Load .skriptrc.toml from workspace root (if present).
        if let Some(root_uri) = params
            .root_uri
            .as_ref()
            .or_else(|| {
                params
                    .workspace_folders
                    .as_ref()
                    .and_then(|f| f.first())
                    .map(|f| &f.uri)
            })
            .and_then(|u| u.to_file_path().ok())
        {
            let mut path = root_uri;
            path.push(".skriptrc.toml");
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => match toml::from_str::<SkriptRcToml>(&content) {
                        Ok(rc) => {
                            let mut cfg = self.state.config();
                            cfg.extra_addons = rc.addons;
                            if let Some(pd) = rc
                                .parser_diagnostics
                                .as_deref()
                                .and_then(ParserDiagnostics::parse)
                            {
                                cfg.parser_diagnostics = pd;
                            }
                            self.state.set_config(cfg);
                            tracing::info!("loaded .skriptrc.toml");
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "failed to parse .skriptrc.toml; ignoring");
                        }
                    },
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to read .skriptrc.toml; ignoring");
                    }
                }
            }
        }

        // 2. Apply initializationOptions (highest precedence).
        self.apply_init_options(&params);

        // 3. Build the pattern engine unconditionally — off the request
        //    thread (compiling ~13.7k patterns takes a moment) — and report
        //    the outcome to the client at startup. The engine supplements the
        //    AST validator with line-based diagnostics; until it is ready,
        //    only AST diagnostics are published, then all open documents are
        //    re-validated.
        {
            let state = self.state.clone();
            let client = self.client.clone();
            tokio::spawn(async move {
                let build_state = state.clone();
                let failures = tokio::task::spawn_blocking(move || build_state.ensure_engine())
                    .await
                    .unwrap_or(usize::MAX);
                if failures == usize::MAX {
                    client
                        .log_message(
                            tower_lsp::lsp_types::MessageType::ERROR,
                            "skript-lsp: pattern engine unavailable \
                             (embedded corpus failed to load, or the pattern \
                             compiler panicked)"
                                .to_owned(),
                        )
                        .await;
                    return;
                }
                let patterns = state
                    .engine_stats()
                    .map(|s| s.patterns_compiled)
                    .unwrap_or(0);
                client
                    .log_message(
                        tower_lsp::lsp_types::MessageType::INFO,
                        format!(
                            "skript-lsp: pattern engine ready \
                             ({patterns} patterns, {failures} compile failures)"
                        ),
                    )
                    .await;
                // The engine just became ready — re-publish merged
                // diagnostics for every open document so the pattern-engine
                // line diagnostics show up alongside the AST ones.
                let uris = state.doc_uris();
                for uri in uris {
                    publish_merged_diagnostics(&state, &client, &uri).await;
                }
            });
        }

        let capabilities = ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            completion_provider: Some(tower_lsp::lsp_types::CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec![" ".to_owned()]),
                ..Default::default()
            }),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                    legend: SemanticTokensLegend {
                        token_types: TOKEN_LEGEND.to_vec(),
                        token_modifiers: vec![],
                    },
                    full: Some(SemanticTokensFullOptions::Bool(true)),
                    range: Some(false),
                    ..Default::default()
                }),
            ),
            document_symbol_provider: Some(OneOf::Left(true)),
            folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
            definition_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            signature_help_provider: Some(SignatureHelpOptions {
                trigger_characters: Some(vec!["(".to_owned(), ",".to_owned()]),
                retrigger_characters: Some(vec![",".to_owned()]),
                ..Default::default()
            }),
            inlay_hint_provider: Some(OneOf::Left(true)),
            rename_provider: Some(OneOf::Left(true)),
            document_highlight_provider: Some(OneOf::Left(true)),
            selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
            // Explicitly NOT advertising: formatting.
            ..Default::default()
        };

        Ok(InitializeResult {
            capabilities,
            server_info: Some(tower_lsp::lsp_types::ServerInfo {
                name: "skript-lsp".to_owned(),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        tracing::info!("client initialized; loading embedded syntax data");
        self.state.spawn_fetch();
        // Once the index becomes ready, re-publish diagnostics for every
        // already-open document (those opened before data loaded).
        let backend = Backend {
            client: self.client.clone(),
            state: self.state.clone(),
        };
        tokio::spawn(async move {
            // Poll readiness; cheap and bounded — readiness flips once.
            for _ in 0..600 {
                if backend.state.ready() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            if backend.state.ready() {
                backend.publish_diagnostics_for_all().await;
            }
        });
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.state.open_doc(uri.clone(), params.text_document.text);
        self.publish_diagnostics(&uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        // FULL sync: the last change in the notification is the whole document.
        if let Some(change) = params.content_changes.into_iter().next() {
            self.state.update_doc(&uri, change.text);
        }
        self.publish_diagnostics(&uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.state.close_doc(&uri);
    }

    async fn completion(
        &self,
        params: tower_lsp::lsp_types::CompletionParams,
    ) -> RpcResult<Option<CompletionResponse>> {
        if !self.state.ready() {
            return Ok(None);
        }
        let uri = params.text_document_position.text_document.uri.to_string();

        let Some(text) = self.state.get_doc(&uri) else {
            return Ok(None);
        };
        let pos = params.text_document_position.position;
        let line_idx = pos.line as usize;
        let lines: Vec<&str> = text.lines().collect();
        let Some(line) = lines.get(line_idx) else {
            return Ok(None);
        };
        // prefix = text on the line from column 0 up to the cursor. The
        // client column is UTF-16; the slice index below is bytes.
        let byte_idx = utf16_col_to_byte_offset(line, pos.character);
        let prefix = &line[..byte_idx];
        let ctx = infer_context(line);

        let max = self.state.config().max_completions;
        let mut items: Vec<skript_core::completion::CompletionItem> = self
            .state
            .with_index(|table| completion::completions(prefix, ctx, table, max))
            .unwrap_or_default();

        // Add AST-aware completions in body context using cached snapshot
        if ctx == CompletionContext::Body {
            if let Some(snap_items) = self.state.with_snapshot(&uri, |snap| {
                let mut extra = Vec::new();
                extra.extend(completion::complete_keywords(prefix));
                extra.extend(completion::complete_variables(
                    prefix,
                    &snap.analysis,
                    &text,
                    pos.line,
                ));
                extra.extend(completion::complete_functions(
                    prefix,
                    &snap.analysis.function_registry,
                ));
                extra
            }) {
                items.extend(snap_items);
            }
        }

        // Deduplicate by label+insert_text
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        items.retain(|it| seen.insert(format!("{}|{}", it.label, it.insert_text)));

        // Sort: sort_bucket asc, usage_score desc, label asc
        items.sort_by(|a, b| {
            a.sort_bucket
                .cmp(&b.sort_bucket)
                .then_with(|| {
                    b.usage_score
                        .partial_cmp(&a.usage_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.label.cmp(&b.label))
        });
        items.truncate(max);

        let lsp_items: Vec<CompletionItem> = items
            .into_iter()
            .map(|it| CompletionItem {
                label: it.label,
                kind: Some(CompletionItemKind::SNIPPET),
                detail: Some(it.detail),
                documentation: Some(tower_lsp::lsp_types::Documentation::String(
                    it.documentation,
                )),
                insert_text: Some(it.insert_text),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                filter_text: Some(it.filter_text),
                ..Default::default()
            })
            .collect();

        Ok(Some(CompletionResponse::Array(lsp_items)))
    }

    async fn hover(&self, params: tower_lsp::lsp_types::HoverParams) -> RpcResult<Option<Hover>> {
        if !self.state.ready() {
            return Ok(None);
        }
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let Some(text) = self.state.get_doc(&uri) else {
            return Ok(None);
        };
        let pos = params.text_document_position_params.position;
        let line_idx = pos.line as usize;
        let lines: Vec<&str> = text.lines().collect();
        let Some(line) = lines.get(line_idx) else {
            return Ok(None);
        };

        // The engine and AST resolver both index the line in UTF-8 bytes.
        let cursor = utf16_col_to_byte_offset(line, pos.character);

        // 1. Syntax docs from SkriptHub via the pattern engine thread.
        //    The engine matches the line against all 13,661 compiled patterns
        //    and returns the best-matching entry ID. We resolve it to a
        //    SyntaxEntry via EntryTable for markdown rendering. The engine's
        //    span covers the whole line; highlight just the word under the
        //    cursor (fall back to the line span on non-word text).
        let syntax_hover = match self
            .state
            .hover_via_engine(line.to_string(), Some(cursor))
            .await
        {
            Some((entry_id, span)) => self
                .state
                .with_index(|table| {
                    table.get(entry_id).map(|entry| {
                        let (s, e) = hover::word_span_at(line, cursor).unwrap_or(span);
                        hover::HoverInfo {
                            markdown: hover::render(entry),
                            range: (s as u32, e as u32),
                        }
                    })
                })
                .flatten(),
            None => None,
        };

        // 2. AST-resolved hover using cached snapshot (takes priority — more precise).
        //    `hover_at_ast` takes a byte column and returns byte columns.
        let ast_hover = self
            .state
            .with_snapshot(&uri, |snap| {
                hover::hover_at_ast(
                    &text,
                    pos.line,
                    cursor as u32,
                    &snap.parse_result.program,
                    &snap.analysis,
                )
            })
            .flatten();

        let info = ast_hover
            .map(|a| hover::HoverInfo {
                markdown: a.markdown,
                range: a.range,
            })
            .or(syntax_hover);

        let Some(info) = info else {
            return Ok(None);
        };

        // `info.range` is a byte span on `line`; convert to UTF-16 for the
        // client. Clamping guarantees a non-inverted range even when the
        // resolver reports a byte inside a multi-byte character.
        let start = byte_offset_to_utf16_col(line, info.range.0 as usize);
        let end = byte_offset_to_utf16_col(line, info.range.1 as usize).max(start);
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: info.markdown,
            }),
            range: Some(Range::new(
                Position::new(pos.line, start),
                Position::new(pos.line, end),
            )),
        }))
    }

    async fn semantic_tokens_full(
        &self,
        params: tower_lsp::lsp_types::SemanticTokensParams,
    ) -> RpcResult<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.to_string();
        let Some(text) = self.state.get_doc(&uri) else {
            return Ok(None);
        };
        let toks = tokens::tokenize(&text);
        let data = encode_semantic_tokens(&text, &toks);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> RpcResult<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.to_string();
        let Some(text) = self.state.get_doc(&uri) else {
            return Ok(None);
        };
        let result = self
            .state
            .with_snapshot(&uri, |s| {
                let program = &s.parse_result.program;
                let mut symbols = Vec::new();
                for item in &program.items {
                    if let Some(sym) = top_level_to_document_symbol(item, &text) {
                        symbols.push(sym);
                    }
                }
                symbols
            })
            .unwrap_or_default();
        Ok(Some(DocumentSymbolResponse::Nested(result)))
    }

    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> RpcResult<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri.to_string();
        let Some(text) = self.state.get_doc(&uri) else {
            return Ok(None);
        };
        let ranges = folding::compute_folding_ranges(&text);
        Ok(Some(ranges))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> RpcResult<Option<GotoDefinitionResponse>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let Some(text) = self.state.get_doc(&uri) else {
            return Ok(None);
        };
        let pos = params.text_document_position_params.position;

        // Resolve the symbol at the cursor via the cached snapshot, then jump
        // to its declaration span. Falls back to `None` (no definition found)
        // when the position isn't a resolvable reference/declaration.
        let location = self
            .state
            .with_snapshot(&uri, |snap| {
                let offset = lsp_position_to_offset(&text, pos.line, pos.character);
                let symbol = references::find_symbol_at_offset(
                    &snap.parse_result.program,
                    &snap.analysis,
                    offset,
                )?;
                Some(Location {
                    uri: params
                        .text_document_position_params
                        .text_document
                        .uri
                        .clone(),
                    range: span_to_range(&text, symbol.declaration_span),
                })
            })
            .flatten();

        Ok(location.map(GotoDefinitionResponse::Scalar))
    }

    async fn references(&self, params: ReferenceParams) -> RpcResult<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let Some(text) = self.state.get_doc(&uri) else {
            return Ok(None);
        };
        let pos = params.text_document_position.position;

        // Resolve the symbol at the cursor via the cached snapshot, then walk
        // the program for every occurrence of its name. The returned spans
        // include the declaration occurrence itself. Falls back to `None` (no
        // references) when the position isn't a resolvable reference.
        let locations = self
            .state
            .with_snapshot(&uri, |snap| {
                let offset = lsp_position_to_offset(&text, pos.line, pos.character);
                let symbol = references::find_symbol_at_offset(
                    &snap.parse_result.program,
                    &snap.analysis,
                    offset,
                )?;
                let spans = references::find_references(&snap.parse_result.program, &symbol);
                Some(
                    spans
                        .iter()
                        .map(|span| Location {
                            uri: params.text_document_position.text_document.uri.clone(),
                            range: span_to_range(&text, *span),
                        })
                        .collect(),
                )
            })
            .flatten();

        Ok(locations)
    }

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> RpcResult<Option<SignatureHelp>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let pos = params.text_document_position_params.position;
        let Some(text) = self.state.get_doc(&uri) else {
            return Ok(None);
        };
        let offset = lsp_position_to_offset(&text, pos.line, pos.character);

        let (func_name, active_param) = match find_function_call_context(&text, offset) {
            Some(ctx) => ctx,
            None => return Ok(None),
        };

        // Resolve the called function via the cached snapshot and build a
        // signature from its declared parameters. Falls back to `None` when
        // the cursor isn't inside a known user-defined function call.
        let info = self
            .state
            .with_snapshot(&uri, |snap| {
                let func = snap.analysis.function_registry.get(&func_name)?;
                let sig_params: Vec<ParameterInformation> = func
                    .decl
                    .parameters
                    .iter()
                    .map(|p| ParameterInformation {
                        label: ParameterLabel::Simple(p.name.clone()),
                        documentation: None,
                    })
                    .collect();
                let param_labels: Vec<String> = func
                    .decl
                    .parameters
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
                let signature = SignatureInformation {
                    label: format!("{}({})", func.decl.name, param_labels.join(", ")),
                    documentation: None,
                    parameters: Some(sig_params),
                    active_parameter: if func.decl.parameters.is_empty() {
                        Some(0)
                    } else {
                        Some(std::cmp::min(
                            active_param,
                            func.decl.parameters.len() as u32 - 1,
                        ))
                    },
                };
                Some(SignatureHelp {
                    signatures: vec![signature],
                    active_signature: Some(0),
                    active_parameter: Some(0),
                })
            })
            .flatten();

        Ok(info)
    }

    async fn code_action(&self, params: CodeActionParams) -> RpcResult<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.to_string();
        let Some(text) = self.state.get_doc(&uri) else {
            return Ok(None);
        };

        let mut actions: Vec<CodeActionOrCommand> = Vec::new();

        // Turn each `unused-variable` diagnostic (see validation.rs) into a
        // quickfix that deletes the declaration line. Replacing the line
        // range with "" keeps the line's newline, leaving a blank line —
        // plan-mandated behavior.
        for diag in &params.context.diagnostics {
            let is_unused = diag.code.as_ref().is_some_and(|c| {
                matches!(
                    c,
                    tower_lsp::lsp_types::NumberOrString::String(s) if s == "unused-variable"
                )
            });
            if !is_unused {
                continue;
            }
            // Message format: "Unused local variable: {name}"
            let var_name = diag
                .message
                .strip_prefix("Unused local variable: ")
                .unwrap_or("");
            if var_name.is_empty() {
                continue;
            }
            let line = diag.range.start.line;
            let line_text = line_text_at(&text, line).unwrap_or("");
            let line_range = Range::new(
                Position::new(line, 0),
                // The edit covers the whole line; report its UTF-16 length,
                // not the UTF-8 byte length.
                Position::new(line, byte_offset_to_utf16_col(line_text, line_text.len())),
            );

            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Remove unused variable {var_name}"),
                kind: Some(CodeActionKind::QUICKFIX),
                edit: Some(WorkspaceEdit {
                    changes: Some(std::collections::HashMap::from([(
                        params.text_document.uri.clone(),
                        vec![TextEdit {
                            range: line_range,
                            new_text: String::new(),
                        }],
                    )])),
                    ..Default::default()
                }),
                diagnostics: Some(vec![diag.clone()]),
                ..Default::default()
            }));
        }

        Ok(Some(actions))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> RpcResult<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri.to_string();
        let Some(text) = self.state.get_doc(&uri) else {
            return Ok(None);
        };

        // Walk the whole document (params.range is intentionally ignored) and
        // collect one type hint per variable assignment with a known inferred
        // type. No snapshot (document not yet analyzed) degrades to no hints.
        let hints = self
            .state
            .with_snapshot(&uri, |snap| {
                let mut hints = Vec::new();
                for item in &snap.parse_result.program.items {
                    collect_type_hints(item, &snap.analysis, &text, &mut hints);
                }
                hints
            })
            .unwrap_or_default();

        Ok(Some(hints))
    }

    async fn rename(
        &self,
        params: tower_lsp::lsp_types::RenameParams,
    ) -> RpcResult<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let Some(text) = self.state.get_doc(&uri) else {
            return Ok(None);
        };
        let pos = params.text_document_position.position;
        let new_name = params.new_name;

        // Resolve the symbol at the cursor, then find all references.
        // Each reference span becomes a TextEdit replacing the old name
        // with the new one. Variable names in Skript are wrapped in
        // `{...}`; the user types the inner name, so we strip/restore
        // braces as needed.
        let edits = self
            .state
            .with_snapshot(&uri, |snap| {
                let offset = lsp_position_to_offset(&text, pos.line, pos.character);
                let symbol = references::find_symbol_at_offset(
                    &snap.parse_result.program,
                    &snap.analysis,
                    offset,
                )?;
                let spans = references::find_references(&snap.parse_result.program, &symbol);

                let doc_uri = params.text_document_position.text_document.uri.clone();
                let text_edits: Vec<TextEdit> = spans
                    .into_iter()
                    .map(|span| {
                        let range = span_to_range(&text, span);
                        let replacement = match symbol.kind {
                            references::SymbolKind::Variable => {
                                // Variables are `{name}` — the span covers
                                // the braces; the user types just the name.
                                format!("{{{new_name}}}")
                            }
                            _ => new_name.clone(),
                        };
                        TextEdit::new(range, replacement)
                    })
                    .collect();
                let mut changes = std::collections::HashMap::new();
                changes.insert(doc_uri, text_edits);
                Some(WorkspaceEdit::new(changes))
            })
            .flatten();

        Ok(edits)
    }

    async fn document_highlight(
        &self,
        params: tower_lsp::lsp_types::DocumentHighlightParams,
    ) -> RpcResult<Option<Vec<tower_lsp::lsp_types::DocumentHighlight>>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let Some(text) = self.state.get_doc(&uri) else {
            return Ok(None);
        };
        let pos = params.text_document_position_params.position;

        // Highlight all occurrences of the symbol under the cursor.
        let highlights = self
            .state
            .with_snapshot(&uri, |snap| {
                let offset = lsp_position_to_offset(&text, pos.line, pos.character);
                let symbol = references::find_symbol_at_offset(
                    &snap.parse_result.program,
                    &snap.analysis,
                    offset,
                )?;
                let spans = references::find_references(&snap.parse_result.program, &symbol);
                Some(
                    spans
                        .into_iter()
                        .map(|span| tower_lsp::lsp_types::DocumentHighlight {
                            range: span_to_range(&text, span),
                            kind: Some(tower_lsp::lsp_types::DocumentHighlightKind::READ),
                        })
                        .collect(),
                )
            })
            .flatten();

        Ok(highlights)
    }

    async fn selection_range(
        &self,
        params: tower_lsp::lsp_types::SelectionRangeParams,
    ) -> RpcResult<Option<Vec<tower_lsp::lsp_types::SelectionRange>>> {
        let uri = params.text_document.uri.to_string();
        let Some(text) = self.state.get_doc(&uri) else {
            return Ok(None);
        };

        // For each requested position, build a chain of progressively
        // wider selections: character → word → line → full document.
        // This is a structural selection (not AST-based) — good enough
        // for the common "expand selection" keystroke (Ctrl+Shift+→).
        let results: Vec<tower_lsp::lsp_types::SelectionRange> = params
            .positions
            .into_iter()
            .map(|pos| {
                let line_text = line_text_at(&text, pos.line).unwrap_or("");
                // The client column is UTF-16; all slicing below is bytes.
                // `utf16_col_to_byte_offset` never returns a mid-character
                // index, so no slice can panic.
                let byte = utf16_col_to_byte_offset(line_text, pos.character);

                // Word range: expand left/right to word boundaries
                // (Unicode alphanumeric + underscore).
                let word_start = line_text[..byte]
                    .char_indices()
                    .rev()
                    .take_while(|(_, c)| is_word_char(*c))
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(byte);
                let word_end = line_text[byte..]
                    .char_indices()
                    .take_while(|(_, c)| is_word_char(*c))
                    .last()
                    .map(|(i, c)| byte + i + c.len_utf8())
                    .unwrap_or(byte);

                let word_range = Range::new(
                    Position::new(pos.line, byte_offset_to_utf16_col(line_text, word_start)),
                    Position::new(pos.line, byte_offset_to_utf16_col(line_text, word_end)),
                );
                let line_range = Range::new(
                    Position::new(pos.line, 0),
                    Position::new(
                        pos.line,
                        byte_offset_to_utf16_col(line_text, line_text.len()),
                    ),
                );
                let doc_range = Range::new(Position::new(0, 0), document_end_position(&text));

                // Chain: word → line → document
                let doc_sel = tower_lsp::lsp_types::SelectionRange {
                    range: doc_range,
                    parent: None,
                };
                let line_sel = tower_lsp::lsp_types::SelectionRange {
                    range: line_range,
                    parent: Some(Box::new(doc_sel)),
                };
                tower_lsp::lsp_types::SelectionRange {
                    range: word_range,
                    parent: Some(Box::new(line_sel)),
                }
            })
            .collect();

        Ok(Some(results))
    }

    async fn shutdown(&self) -> RpcResult<()> {
        tracing::info!("shutdown requested");
        Ok(())
    }
}

/// Text of a 0-based line, with the trailing `\r` of CRLF removed.
fn line_text_at(text: &str, line: u32) -> Option<&str> {
    text.lines().nth(line as usize)
}

/// Byte offset of the first character of a 0-based line.
fn line_start_offset(text: &str, line: u32) -> Option<usize> {
    if line == 0 {
        return Some(0);
    }
    let mut current = 0u32;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            current += 1;
            if current == line {
                return Some(i + 1);
            }
        }
    }
    None
}

/// Convert an LSP position to a UTF-8 byte offset into `text`.
///
/// The client column is UTF-16 code units; all core APIs index bytes.
/// Positions past the end of the document clamp to `text.len()`, matching
/// the graceful-degradation behavior of the handlers that use this.
fn lsp_position_to_offset(text: &str, line: u32, character: u32) -> usize {
    let Some(start) = line_start_offset(text, line) else {
        return text.len();
    };
    let rest = &text[start..];
    let raw_line = rest.split('\n').next().unwrap_or("");
    let line_text = raw_line.strip_suffix('\r').unwrap_or(raw_line);
    start + utf16_col_to_byte_offset(line_text, character)
}

/// Convert a byte offset into `text` to an LSP position (UTF-16 column).
fn offset_to_lsp_position(text: &str, offset: usize) -> Position {
    let (line, byte_col) = validation::offset_to_linecol(text, offset);
    let line_text = line_text_at(text, line).unwrap_or("");
    Position::new(line, byte_offset_to_utf16_col(line_text, byte_col as usize))
}

/// Convert a 0-based character column (Unicode scalar values) to a byte
/// offset into `line`. Used for `skript-checks` diagnostics.
fn char_col_to_byte_offset(line: &str, col: usize) -> usize {
    line.char_indices()
        .nth(col)
        .map(|(i, _)| i)
        .unwrap_or(line.len())
}

/// A "word" for selectionRange expansion: Unicode alphanumeric + `_`.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// End position of the whole document in LSP coordinates.
///
/// Uses the number of newlines (not `str::lines`, which drops a trailing
/// empty line) so a document ending in `\n` correctly ends at column 0 of
/// the following empty line.
fn document_end_position(text: &str) -> Position {
    if text.is_empty() {
        return Position::new(0, 0);
    }
    let line = text.bytes().filter(|&b| b == b'\n').count() as u32;
    let last = text.rsplit('\n').next().unwrap_or("");
    let last = last.strip_suffix('\r').unwrap_or(last);
    Position::new(line, byte_offset_to_utf16_col(last, last.len()))
}

/// Decide whether a line is at the top level (no indentation) of the file or
/// inside an event/function/command body (indented).
fn infer_context(line: &str) -> CompletionContext {
    // Skript bodies are indented; top-level statements are flush-left.
    if line.starts_with(char::is_whitespace) {
        CompletionContext::Body
    } else {
        CompletionContext::TopLevel
    }
}

/// Convert a byte-offset `Span` to an LSP `Range` (UTF-16 columns).
fn span_to_range(source: &str, span: skript_core::ast::Span) -> Range {
    Range::new(
        offset_to_lsp_position(source, span.start),
        offset_to_lsp_position(source, span.end),
    )
}

/// Convert a `TopLevelItem` into an LSP `DocumentSymbol` for the outline view.
fn top_level_to_document_symbol(
    item: &skript_core::ast::TopLevelItem,
    source: &str,
) -> Option<DocumentSymbol> {
    match item {
        skript_core::ast::TopLevelItem::Function(f) => {
            let range = span_to_range(source, f.span);
            Some(DocumentSymbol {
                name: f.name.clone(),
                detail: Some(format!(
                    "function({})",
                    f.parameters
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
                kind: SymbolKind::FUNCTION,
                range,
                selection_range: range,
                children: Some(vec![]),
                #[allow(deprecated)]
                deprecated: None,
                tags: None,
            })
        }
        skript_core::ast::TopLevelItem::Command(c) => {
            let range = span_to_range(source, c.span);
            Some(DocumentSymbol {
                name: c.name.clone(),
                detail: Some("command".to_owned()),
                kind: SymbolKind::FUNCTION,
                range,
                selection_range: range,
                children: Some(vec![]),
                #[allow(deprecated)]
                deprecated: None,
                tags: None,
            })
        }
        skript_core::ast::TopLevelItem::Event(e) => {
            let range = span_to_range(source, e.span);
            Some(DocumentSymbol {
                name: format!("on {}", e.event_type),
                detail: Some("event".to_owned()),
                kind: SymbolKind::EVENT,
                range,
                selection_range: range,
                children: Some(vec![]),
                #[allow(deprecated)]
                deprecated: None,
                tags: None,
            })
        }
        skript_core::ast::TopLevelItem::PeriodicEvent(e) => {
            let range = span_to_range(source, e.span);
            Some(DocumentSymbol {
                name: format!("every {}", e.period),
                detail: Some("periodic event".to_owned()),
                kind: SymbolKind::EVENT,
                range,
                selection_range: range,
                children: Some(vec![]),
                #[allow(deprecated)]
                deprecated: None,
                tags: None,
            })
        }
        _ => None,
    }
}

/// Collect inlay type hints for every assignment inside a top-level item.
///
/// Uses [`skript_core::ast::Visitor`] to walk the whole statement tree —
/// including statements nested inside sections such as function/event bodies,
/// if/else branches, and loops — and emits one hint per assignment whose
/// target is a plain variable and whose value has a known inferred type. The
/// hint label is the type, rendered right after the variable name
/// (`set {_msg}: text to "hi"`).
fn collect_type_hints(
    item: &skript_core::ast::TopLevelItem,
    analysis: &skript_core::semantic::SemanticAnalysis,
    source: &str,
    hints: &mut Vec<InlayHint>,
) {
    /// Visitor collecting hints for `Statement::Assignment` nodes. Delegates
    /// to [`skript_core::ast::walk_statement`] after each statement so the
    /// walk keeps recursing into nested sections.
    struct TypeHintCollector<'a> {
        analysis: &'a skript_core::semantic::SemanticAnalysis,
        source: &'a str,
        hints: &'a mut Vec<InlayHint>,
    }

    impl skript_core::ast::Visitor for TypeHintCollector<'_> {
        fn visit_statement(&mut self, statement: &skript_core::ast::Statement) {
            if let skript_core::ast::Statement::Assignment(assignment) = statement {
                if let skript_core::ast::AssignmentTarget::Variable(variable) = &assignment.target {
                    // The type inference table is keyed by the value
                    // expression's span start; unknown expressions map to
                    // `TypeInfo::Unknown` and are skipped.
                    let ty = self
                        .analysis
                        .type_inference
                        .get_type(assignment.value.span().start);
                    if let skript_core::semantic::types::TypeInfo::Known(ty) = ty {
                        // Hint renders right after the variable name. The
                        // span is a byte offset; convert to UTF-16 for LSP.
                        let position = offset_to_lsp_position(self.source, variable.span.end);
                        self.hints.push(InlayHint {
                            position,
                            label: InlayHintLabel::String(format!(": {ty}")),
                            kind: Some(InlayHintKind::TYPE),
                            text_edits: None,
                            tooltip: None,
                            padding_left: None,
                            padding_right: None,
                            data: None,
                        });
                    }
                }
            }
            skript_core::ast::walk_statement(self, statement);
        }
    }

    let mut visitor = TypeHintCollector {
        analysis,
        source,
        hints,
    };
    visitor.visit_top_level_item(item);
}

/// Encode semantic tokens in the LSP relative-position delta format.
///
/// `tokenize` reports UTF-8 byte columns/lengths; the LSP wire format uses
/// UTF-16 code units, so every token is converted against its source line
/// (and clamped so a token can never extend past the line's true end).
fn encode_semantic_tokens(
    source: &str,
    toks: &[skript_core::tokens::SemanticToken],
) -> Vec<SemanticToken> {
    let mut out = Vec::with_capacity(toks.len());
    let mut prev_line = 0u32;
    let mut prev_col = 0u32;
    for t in toks {
        let line_text = line_text_at(source, t.line).unwrap_or("");
        let start_byte = (t.col as usize).min(line_text.len());
        let end_byte = start_byte
            .saturating_add(t.length as usize)
            .min(line_text.len());
        let start = byte_offset_to_utf16_col(line_text, start_byte);
        let end = byte_offset_to_utf16_col(line_text, end_byte).max(start);
        let delta_line = t.line.saturating_sub(prev_line);
        let delta_col = if delta_line == 0 {
            start.saturating_sub(prev_col)
        } else {
            start
        };
        out.push(SemanticToken {
            delta_line,
            delta_start: delta_col,
            length: end - start,
            token_type: token_type_index(t.ty),
            token_modifiers_bitset: 0,
        });
        prev_line = t.line;
        prev_col = start;
    }
    out
}

/// Given source text and cursor offset, find the enclosing function call.
/// Returns `(function_name, active_parameter_index)` if found.
///
/// Walks backward from the cursor counting parens; the innermost `(` that
/// isn't closed by a later `)` is the call's opening paren. The active
/// parameter is the number of commas between that paren and the cursor.
fn find_function_call_context(source: &str, offset: usize) -> Option<(String, u32)> {
    let text = &source[..offset];
    let mut depth = 0i32;
    let mut call_start = None;

    for (i, c) in text.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' => {
                if depth <= 0 {
                    call_start = Some(i);
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }

    let call_start = call_start?;

    // Extract function name: word characters before '('.
    let before = &text[..call_start];
    let func_name = before
        .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
        .next()?;
    if func_name.is_empty() {
        return None;
    }

    // Count commas between '(' and cursor.
    let args = &source[call_start + 1..offset];
    let active_param = args.chars().filter(|c| *c == ',').count() as u32;

    Some((func_name.to_owned(), active_param))
}

#[cfg(test)]
mod tests {
    use super::*;
    use skript_checks::{LineDiagnostic, Severity as CheckSeverity};

    fn line_diag(col_start: usize, col_end: usize) -> LineDiagnostic {
        LineDiagnostic {
            line: 0,
            col_start,
            col_end,
            severity: CheckSeverity::Info,
            code: "x",
            message: String::new(),
        }
    }

    #[test]
    fn line_diagnostics_report_utf16_columns_not_bytes() {
        let source = "broadcast §a§b§c§dhello world";
        // Character columns from skript-checks: 14 = 'h', 19 = after 'o'.
        let lsp = line_diagnostic_to_lsp(line_diag(14, 19), source);
        assert_eq!(lsp.range.start, Position::new(0, 14));
        assert_eq!(lsp.range.end, Position::new(0, 19));

        // The same columns are byte offsets 22 and 27; the pre-fix code
        // emitted those directly and overshot the line.
        assert_eq!(source.len(), 33);
        assert_eq!(source.encode_utf16().count(), 29);
    }

    #[test]
    fn line_diagnostics_clamp_out_of_range_columns() {
        let lsp = line_diagnostic_to_lsp(line_diag(99, 100), "§§");
        assert_eq!(lsp.range.start, Position::new(0, 2));
        assert_eq!(lsp.range.end, Position::new(0, 2));
    }

    #[test]
    fn semantic_token_columns_are_utf16_and_never_exceed_the_line() {
        let source = "broadcast §a§b§c§dhello world";
        let utf16_len = source.encode_utf16().count() as u32;
        let encoded = encode_semantic_tokens(source, &skript_core::tokens::tokenize(source));

        let mut line = 0u32;
        let mut col = 0u32;
        for token in &encoded {
            line += token.delta_line;
            col = if token.delta_line == 0 {
                col + token.delta_start
            } else {
                token.delta_start
            };
            assert_eq!(line, 0);
            assert!(
                col + token.length <= utf16_len,
                "token {col}..{} exceeds line length {utf16_len}",
                col + token.length
            );
        }
    }

    #[test]
    fn positions_round_trip_through_non_ascii_text() {
        let source = "on load:\n\tbroadcast §a§b §c hello\n\tbroadcast \"😀\"";
        let positions = [
            Position::new(1, 0),
            Position::new(1, 8),  // tab + "broadcast" (9 bytes, 9 units)
            Position::new(1, 10), // space before the first colour code
            Position::new(1, 19), // 'h' of hello
            Position::new(2, 14), // inside the emoji line
            Position::new(2, 15), // line end
        ];
        for pos in positions {
            let offset = lsp_position_to_offset(source, pos.line, pos.character);
            let back = offset_to_lsp_position(source, offset);
            assert_eq!(back, pos, "round trip failed for {pos:?}");
        }
    }

    #[test]
    fn document_end_position_accounts_for_trailing_newline_and_utf16() {
        assert_eq!(document_end_position(""), Position::new(0, 0));
        assert_eq!(document_end_position("a"), Position::new(0, 1));
        assert_eq!(document_end_position("a\n"), Position::new(1, 0));
        assert_eq!(document_end_position("a\n§"), Position::new(1, 1));
        assert_eq!(document_end_position("a\r\nb"), Position::new(1, 1));
        assert_eq!(document_end_position("😀"), Position::new(0, 2));
    }

    #[test]
    fn finds_call_and_active_parameter() {
        let src = "greet(\"hi\", name)";
        // Cursor right after the second argument name.
        let offset = src.find("name").unwrap() + "name".len();
        assert_eq!(
            find_function_call_context(src, offset),
            Some(("greet".to_owned(), 1))
        );
    }

    #[test]
    fn cursor_inside_nested_call_reports_inner_function() {
        let src = "outer(inner(x)";
        // Cursor right after the inner argument `x`.
        let offset = src.find('x').unwrap() + 1;
        assert_eq!(
            find_function_call_context(src, offset),
            Some(("inner".to_owned(), 0))
        );
    }

    #[test]
    fn cursor_in_outer_args_after_closed_inner_call_reports_outer() {
        let src = "outer(inner(x), y)";
        // Cursor after `y`, still inside the outer call.
        let offset = src.find('y').unwrap() + 1;
        assert_eq!(
            find_function_call_context(src, offset),
            Some(("outer".to_owned(), 1))
        );
    }

    #[test]
    fn returns_none_outside_a_call() {
        // No parens at all.
        assert_eq!(find_function_call_context("on load:", 4), None);
        // Cursor after the closing paren is not inside the call.
        assert_eq!(find_function_call_context("foo()", "foo()".len()), None);
        // Empty name before '(' is not a call.
        assert_eq!(find_function_call_context("(x)", 1), None);
    }
}
