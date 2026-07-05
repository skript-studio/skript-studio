use tower_lsp::lsp_types::FoldingRange;

/// Compute folding ranges from source text using indentation tracking.
/// Each Indent/Dedent pair produces a range.
pub fn compute_folding_ranges(source: &str) -> Vec<FoldingRange> {
    // Walk lines, track indentation level by leading whitespace count
    let mut indent_stack: Vec<(u32, u32)> = Vec::new(); // (line_idx, indent_level)
    let mut ranges = Vec::new();
    
    for (line_idx, line) in source.lines().enumerate() {
        let indent = line.chars().take_while(|c| *c == ' ' || *c == '\t').count() as u32;
        let trimmed = line.trim();
        
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        
        // Pop stack while current indent is less than stack top
        while let Some(&(start_line, start_indent)) = indent_stack.last() {
            if indent <= start_indent {
                indent_stack.pop();
                // Emit range if it spans at least 2 lines
                if line_idx as u32 > start_line + 1 {
                    ranges.push(FoldingRange {
                        start_line,
                        end_line: line_idx as u32 - 1,
                        ..Default::default()
                    });
                }
            } else {
                break;
            }
        }
        
        // Current line is indented more → push to stack
        if !trimmed.is_empty() && indent > 0 && indent_stack.last().map_or(true, |&(_, i)| indent > i) {
            indent_stack.push((line_idx as u32, indent));
        }
    }
    
    // Close remaining open ranges
    while let Some((start_line, _)) = indent_stack.pop() {
        let end_line = source.lines().count() as u32 - 1;
        if end_line > start_line + 1 {
            ranges.push(FoldingRange {
                start_line,
                end_line,
                ..Default::default()
            });
        }
    }
    
    ranges
}
