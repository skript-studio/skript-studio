//! UTF-16 ↔ UTF-8 position conversion at the LSP boundary.
//!
//! The LSP specification counts `Position.character` in **UTF-16 code
//! units**, while every `skript-core` and `skript-checks` API in this
//! workspace counts **UTF-8 bytes**. The two agree only on ASCII. These two
//! functions are the single place where the server crosses that boundary:
//! client-supplied columns are converted to bytes before any string is
//! sliced, and every outgoing column is converted back to UTF-16.
//!
//! Both functions clamp instead of panicking. A column that lands inside a
//! surrogate pair or past the end of the line resolves to the nearest
//! character boundary (the start of the character containing that unit for
//! the forward conversion; the previous boundary for the reverse one).

/// Convert an LSP UTF-16 column to a UTF-8 byte offset into `line`.
///
/// Columns inside a multi-unit character are floored to that character's
/// first byte, and columns past the end clamp to `line.len()`.
pub fn utf16_col_to_byte_offset(line: &str, col: u32) -> usize {
    let mut units = 0u32;
    for (byte, ch) in line.char_indices() {
        let next = units + ch.len_utf16() as u32;
        if next > col {
            return byte;
        }
        units = next;
    }
    line.len()
}

/// Convert a UTF-8 byte offset into `line` to an LSP UTF-16 column.
///
/// Offsets that are not on a character boundary are floored to the previous
/// boundary; offsets past the end clamp to the line's full UTF-16 length.
pub fn byte_offset_to_utf16_col(line: &str, byte: usize) -> u32 {
    let mut at = byte.min(line.len());
    while at > 0 && !line.is_char_boundary(at) {
        at -= 1;
    }
    line[..at].encode_utf16().count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(line: &str) {
        let utf16_len = line.encode_utf16().count() as u32;
        assert_eq!(byte_offset_to_utf16_col(line, line.len()), utf16_len);
        for byte in (0..=line.len()).filter(|&b| line.is_char_boundary(b)) {
            let col = byte_offset_to_utf16_col(line, byte);
            assert_eq!(
                utf16_col_to_byte_offset(line, col),
                byte,
                "round trip failed for {:?} at byte {byte} (col {col})",
                line
            );
        }
    }

    #[test]
    fn ascii_columns_are_identity() {
        let line = "broadcast hello world";
        assert_eq!(utf16_col_to_byte_offset(line, 0), 0);
        assert_eq!(utf16_col_to_byte_offset(line, 9), 9);
        assert_eq!(
            utf16_col_to_byte_offset(line, line.len() as u32),
            line.len()
        );
        assert_eq!(byte_offset_to_utf16_col(line, 9), 9);
        round_trip(line);
    }

    #[test]
    fn section_sign_run_uses_one_utf16_unit_per_sign() {
        let line = "§a§b§c§dhello world";
        // 8 one-unit characters occupy 12 bytes; "hello" starts at byte 12.
        assert_eq!(utf16_col_to_byte_offset(line, 8), 12);
        assert_eq!(byte_offset_to_utf16_col(line, 12), 8);
        assert_eq!(byte_offset_to_utf16_col(line, line.len()), 19);
        round_trip(line);
    }

    #[test]
    fn cjk_is_three_bytes_per_unit() {
        let line = "广播 hello";
        // 广 = 3 bytes / 1 unit, 播 = 3 bytes / 1 unit, space = 1.
        assert_eq!(utf16_col_to_byte_offset(line, 0), 0);
        assert_eq!(utf16_col_to_byte_offset(line, 1), 3);
        assert_eq!(utf16_col_to_byte_offset(line, 2), 6);
        assert_eq!(utf16_col_to_byte_offset(line, 3), 7);
        assert_eq!(byte_offset_to_utf16_col(line, 7), 3);
        round_trip(line);
    }

    #[test]
    fn cyrillic_is_two_bytes_per_unit() {
        let line = "привет hello";
        // Each Cyrillic scalar is 2 bytes / 1 UTF-16 unit.
        assert_eq!(utf16_col_to_byte_offset(line, 1), 2);
        assert_eq!(utf16_col_to_byte_offset(line, 6), 12);
        assert_eq!(byte_offset_to_utf16_col(line, 12), 6);
        round_trip(line);
    }

    #[test]
    fn surrogate_pair_occupies_two_utf16_units() {
        let line = "😀 hello";
        // 😀 = 4 bytes / 2 UTF-16 units.
        assert_eq!(utf16_col_to_byte_offset(line, 0), 0);
        assert_eq!(utf16_col_to_byte_offset(line, 2), 4);
        assert_eq!(utf16_col_to_byte_offset(line, 3), 5);
        assert_eq!(byte_offset_to_utf16_col(line, 4), 2);
        round_trip(line);
    }

    #[test]
    fn combining_mark_has_its_own_utf16_unit() {
        // "e" + combining acute: 3 bytes, 2 scalar values, 2 UTF-16 units.
        let line = "e\u{301}x";
        assert_eq!(utf16_col_to_byte_offset(line, 1), 1);
        assert_eq!(utf16_col_to_byte_offset(line, 2), 3);
        assert_eq!(byte_offset_to_utf16_col(line, 3), 2);
        round_trip(line);
    }

    #[test]
    fn columns_inside_a_character_clamp_to_its_start() {
        let line = "😀a";
        // Column 1 is the second surrogate of the emoji, not a boundary.
        assert_eq!(utf16_col_to_byte_offset(line, 1), 0);
        // Column 2 is exactly the boundary after the emoji.
        assert_eq!(utf16_col_to_byte_offset(line, 2), 4);

        let line = "a§b";
        // Every UTF-16 column here is a boundary: `§` is one code unit but
        // two bytes, so column 2 is `b` at byte 3.
        assert_eq!(utf16_col_to_byte_offset(line, 1), 1);
        assert_eq!(utf16_col_to_byte_offset(line, 2), 3);
        assert_eq!(utf16_col_to_byte_offset(line, 3), 4);
    }

    #[test]
    fn byte_offsets_inside_a_character_clamp_to_previous_boundary() {
        assert_eq!(byte_offset_to_utf16_col("§", 1), 0);
        assert_eq!(byte_offset_to_utf16_col("😀", 1), 0);
        assert_eq!(byte_offset_to_utf16_col("😀", 2), 0);
        assert_eq!(byte_offset_to_utf16_col("😀", 3), 0);
        assert_eq!(byte_offset_to_utf16_col("😀", 4), 2);
    }

    #[test]
    fn past_end_clamps_to_full_length() {
        assert_eq!(utf16_col_to_byte_offset("hello", 100), 5);
        assert_eq!(utf16_col_to_byte_offset("😀", 100), 4);
        assert_eq!(byte_offset_to_utf16_col("hello", 100), 5);
        assert_eq!(byte_offset_to_utf16_col("😀", 100), 2);
        assert_eq!(utf16_col_to_byte_offset("", 10), 0);
        assert_eq!(byte_offset_to_utf16_col("", 10), 0);
    }
}
