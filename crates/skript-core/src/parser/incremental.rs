use crate::ast::Program;
use crate::parser::{parse_program, ParseResult};

/// A range of a top-level section in the source.
#[derive(Debug, Clone)]
pub struct Section {
    pub start_offset: usize,
    pub end_offset: usize,
    pub start_line: usize,
    pub end_line: usize,
}

fn line_byte_offset(source: &str, line: usize) -> usize {
    let mut current = 0usize;
    for (i, c) in source.char_indices() {
        if current == line {
            return i;
        }
        if c == '\n' {
            current += 1;
        }
    }
    source.len()
}

/// Split source text into sections at indent-0 boundaries.
/// Each section = a flush-left header line + its indented body lines.
/// Blank lines between sections are included in the preceding section's end.
pub fn split_sections(source: &str) -> Vec<Section> {
    if source.trim().is_empty() {
        return vec![];
    }

    let lines: Vec<&str> = source.lines().collect();
    if lines.is_empty() {
        return vec![];
    }

    let mut sections = Vec::new();
    let mut section_start_line = 0usize;
    let mut section_start_byte = 0usize;
    let mut in_section = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if i == 0 || (!line.starts_with(char::is_whitespace) && !line.is_empty()) {
            if in_section {
                let end_byte = line_byte_offset(source, i);
                sections.push(Section {
                    start_offset: section_start_byte,
                    end_offset: end_byte,
                    start_line: section_start_line,
                    end_line: i,
                });
            }
            section_start_line = i;
            section_start_byte = line_byte_offset(source, i);
            in_section = true;
        }
    }

    if in_section {
        sections.push(Section {
            start_offset: section_start_byte,
            end_offset: source.len(),
            start_line: section_start_line,
            end_line: lines.len(),
        });
    }

    sections
}

/// Incrementally update parse result when source changes.
/// Only re-parses sections whose text actually changed.
pub fn incremental_update(
    old_source: &str,
    new_source: &str,
    old_program: &Program,
) -> ParseResult {
    if old_source == new_source {
        return ParseResult {
            program: old_program.clone(),
            errors: vec![],
        };
    }

    let old_sections = split_sections(old_source);
    let new_sections = split_sections(new_source);

    let mut new_items = Vec::new();
    let mut errors = Vec::new();

    let common = old_sections.len().min(new_sections.len());
    for i in 0..common {
        let old_text = &old_source[old_sections[i].start_offset..old_sections[i].end_offset];
        let new_text = &new_source[new_sections[i].start_offset..new_sections[i].end_offset];

        if old_text == new_text {
            if let Some(item) = old_program.items.get(i) {
                new_items.push(item.clone());
            }
        } else {
            let section_result = parse_program(new_text);
            new_items.extend(section_result.program.items);
            errors.extend(section_result.errors);
        }
    }

    for i in common..new_sections.len() {
        let new_text = &new_source[new_sections[i].start_offset..new_sections[i].end_offset];
        let section_result = parse_program(new_text);
        new_items.extend(section_result.program.items);
        errors.extend(section_result.errors);
    }

    ParseResult {
        program: Program { items: new_items },
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_sections_empty() {
        let sections = split_sections("");
        assert!(sections.is_empty());
    }

    #[test]
    fn test_split_sections_only_whitespace() {
        let sections = split_sections("  \n  \n");
        assert!(sections.is_empty());
    }

    #[test]
    fn test_split_sections_single() {
        let source = "function foo():\n\treturn";
        let sections = split_sections(source);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].start_line, 0);
    }

    #[test]
    fn test_split_sections_multiple() {
        let source = "function foo():\n\treturn\n\non join:\n\tbroadcast \"hi\"";
        let sections = split_sections(source);
        assert_eq!(sections.len(), 2);
        assert!(sections[0].end_offset <= sections[1].start_offset);
    }

    #[test]
    fn test_split_sections_with_comments() {
        let source = "# header\nfunction foo():\n\treturn\n# between\non join:\n\tbroadcast \"hi\"";
        let sections = split_sections(source);
        assert_eq!(sections.len(), 2);
    }

    #[test]
    fn test_incremental_update_no_change() {
        let source = "function foo():\n\treturn";
        let result1 = parse_program(source);
        let result2 = incremental_update(source, source, &result1.program);
        assert_eq!(result2.program.items.len(), result1.program.items.len());
        assert!(result2.errors.is_empty());
    }

    #[test]
    fn test_incremental_update_equals_full_parse() {
        let old_source = "function foo():\n\treturn";
        let new_source = "function foo():\n\tbroadcast \"hello\"\n\treturn";
        let old_result = parse_program(old_source);
        let incr_result = incremental_update(old_source, new_source, &old_result.program);
        let full_result = parse_program(new_source);
        assert_eq!(
            incr_result.program.items.len(),
            full_result.program.items.len()
        );
    }

    #[test]
    fn test_incremental_update_add_section() {
        let old = "function foo():\n\treturn";
        let new = "function foo():\n\treturn\n\non join:\n\tbroadcast \"hi\"";
        let old_result = parse_program(old);
        let incr_result = incremental_update(old, new, &old_result.program);
        assert_eq!(incr_result.program.items.len(), 2);
    }

    #[test]
    fn test_incremental_update_remove_section() {
        let old = "function foo():\n\treturn\n\non join:\n\tbroadcast \"hi\"";
        let new = "function foo():\n\treturn";
        let old_result = parse_program(old);
        let incr_result = incremental_update(old, new, &old_result.program);
        assert_eq!(incr_result.program.items.len(), 1);
    }

    #[test]
    fn test_incremental_update_produces_same_result() {
        let cases = vec![
            (
                "function a():\n\treturn",
                "function a():\n\treturn\n\tbroadcast \"x\"",
            ),
            (
                "function a():\n\treturn\n\non join:\n\tbroadcast \"hi\"",
                "function a():\n\treturn\n\tbroadcast \"x\"\n\non join:\n\tbroadcast \"bye\"",
            ),
            ("", "function a():\n\treturn"),
            ("function a():\n\treturn", ""),
        ];
        for (old, new) in cases {
            let old_result = parse_program(old);
            let incr = incremental_update(old, new, &old_result.program);
            let full = parse_program(new);
            assert_eq!(
                incr.program.items.len(),
                full.program.items.len(),
                "mismatch for old={old:?} new={new:?}"
            );
        }
    }
}
