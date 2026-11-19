use std::io::{self, Read};

#[derive(Debug, Clone)]
struct FormatOptions {
    heading_to_body_spacing: usize,    // 0 or 1, default is 1
    heading_to_heading_spacing: usize, // 0 or 1, default is 1
    wrap_paragraphs: bool,             // default is false
    wrap_lists: bool,                  // default is false
    fill_column: usize,                // default is 80
    left_indent: bool,                 // default is false
    body_to_body_spacing: usize,       // min blank lines between different body elements, 0 disables
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions {
            heading_to_body_spacing: 1,
            heading_to_heading_spacing: 1,
            wrap_paragraphs: false,
            wrap_lists: false,
            fill_column: 80,
            left_indent: false,
            body_to_body_spacing: 0,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    Normal,
    InBlock(String), // Closing tag string, e.g. "#+END_SRC"
    InDrawer { indent: String },
}

fn is_heading_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('*') {
        return false;
    }
    let stars_count = trimmed.chars().take_while(|&c| c == '*').count();
    if stars_count == 0 {
        return false;
    }
    let after_stars = &trimmed[stars_count..];
    if !(after_stars.starts_with(' ') || after_stars.is_empty()) {
        return false;
    }

    // In Org-mode, a line starting with exactly one star and having leading spaces
    // is a list item bullet, not a heading. Headings must start at column 0
    // or have multiple stars (e.g., "** subheading").
    let has_leading_spaces = line.starts_with(' ') || line.starts_with('\t');
    if has_leading_spaces && stars_count == 1 {
        return false;
    }

    true
}

fn get_heading_level(line: &str) -> Option<usize> {
    if !line.starts_with('*') {
        return None;
    }
    let stars_count = line.chars().take_while(|&c| c == '*').count();
    if stars_count == 0 {
        return None;
    }
    let after_stars = &line[stars_count..];
    if after_stars.starts_with(' ') || after_stars.is_empty() {
        Some(stars_count)
    } else {
        None
    }
}

fn is_planning_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    ["SCHEDULED:", "DEADLINE:", "CLOSED:"]
        .iter()
        .any(|keyword| trimmed.starts_with(keyword))
}

fn is_heading_or_drawer_end(line: &str) -> bool {
    is_heading_line(line) || line.trim().ends_with(":END:") || is_planning_line(line)
}

fn is_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();

    // Unordered bullets: -, +, *
    if trimmed.starts_with("-") || trimmed.starts_with("+") || trimmed.starts_with("*") {
        let after_bullet = &trimmed[1..];
        if after_bullet.starts_with(' ')
            || after_bullet.starts_with('\t')
            || after_bullet.is_empty()
        {
            return true;
        }
    }

    // Ordered bullets: digit followed by . or )
    if let Some(first_char) = trimmed.chars().next() {
        if first_char.is_ascii_digit() {
            let digits_count = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
            let after_digits = &trimmed[digits_count..];
            if after_digits.starts_with(".") || after_digits.starts_with(")") {
                let after_marker = &after_digits[1..];
                if after_marker.starts_with(' ')
                    || after_marker.starts_with('\t')
                    || after_marker.is_empty()
                {
                    return true;
                }
            }
        } else if first_char.is_ascii_lowercase() {
            // Check for alphabetical list markers like "a. " or "a) "
            let after_char = &trimmed[1..];
            if after_char.starts_with(".") || after_char.starts_with(")") {
                let after_marker = &after_char[1..];
                if after_marker.starts_with(' ')
                    || after_marker.starts_with('\t')
                    || after_marker.is_empty()
                {
                    return true;
                }
            }
        }
    }
    false
}

fn format_list_line(line: &str) -> String {
    let indent = line
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();
    let trimmed = line.trim_start();
    if trimmed.starts_with("-") {
        let content = trimmed[1..].trim();
        if content.is_empty() {
            format!("{}-", indent)
        } else {
            format!("{}- {}", indent, content)
        }
    } else if trimmed.starts_with("+") {
        let content = trimmed[1..].trim();
        if content.is_empty() {
            format!("{}+", indent)
        } else {
            format!("{}+ {}", indent, content)
        }
    } else if trimmed.starts_with("*") {
        let content = trimmed[1..].trim();
        if content.is_empty() {
            format!("{}*", indent)
        } else {
            format!("{}* {}", indent, content)
        }
    } else {
        let digits_count = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits_count > 0 {
            let after_digits = &trimmed[digits_count..];
            if after_digits.starts_with('.') || after_digits.starts_with(')') {
                let marker = &trimmed[..digits_count + 1];
                let content = trimmed[digits_count + 1..].trim();
                if content.is_empty() {
                    format!("{}{}", indent, marker)
                } else {
                    format!("{}{} {}", indent, marker, content)
                }
            } else {
                line.to_string()
            }
        } else {
            if let Some(first_char) = trimmed.chars().next() {
                if first_char.is_ascii_lowercase() {
                    let after_char = &trimmed[1..];
                    if after_char.starts_with('.') || after_char.starts_with(')') {
                        let marker = &trimmed[..2];
                        let content = trimmed[2..].trim();
                        if content.is_empty() {
                            format!("{}{}", indent, marker)
                        } else {
                            format!("{}{} {}", indent, marker, content)
                        }
                    } else {
                        line.to_string()
                    }
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            }
        }
    }
}

fn is_regular_paragraph_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return false;
    }

    // Check for structural headings
    if is_heading_line(line) {
        return false;
    }

    // Check for list items
    if is_list_item(trimmed) {
        return false;
    }

    // Check for drawers (starts and ends with : and is not end)
    if trimmed.starts_with(':') && trimmed.ends_with(':') {
        return false;
    }
    if trimmed.to_uppercase() == ":END:" {
        return false;
    }

    // Check for block boundaries
    let trimmed_upper = trimmed.to_uppercase();
    if trimmed_upper.starts_with("#+BEGIN_") || trimmed_upper.starts_with("#+END_") {
        return false;
    }

    // Check for metadata/keyword lines
    if trimmed.starts_with("#+") {
        return false;
    }

    // Check for comments
    if trimmed.starts_with('#') {
        return false;
    }

    // Check for tables
    if trimmed.starts_with('|') {
        return false;
    }

    // Check for fixed-width lines
    if trimmed == ":" || trimmed.starts_with(": ") {
        return false;
    }

    // Check for horizontal rules
    let trimmed_end = trimmed.trim_end();
    if trimmed_end.len() >= 5 && trimmed_end.chars().all(|c| c == '-') {
        return false;
    }

    true
}

fn char_width(c: char) -> usize {
    if c == '\t' {
        8
    } else {
        let val = c as u32;
        if (val >= 0x1100 && val <= 0x11FF) || // Hangul Jamo
           (val >= 0x2E80 && val <= 0x2EFF) || // CJK Radicals Supplement
           (val >= 0x3000 && val <= 0x303F) || // CJK Symbols and Punctuation
           (val >= 0x3040 && val <= 0x309F) || // Hiragana
           (val >= 0x30A0 && val <= 0x30FF) || // Katakana
           (val >= 0x3130 && val <= 0x318F) || // Hangul Compatibility Jamo
           (val >= 0x3200 && val <= 0x32FF) || // Enclosed CJK Letters and Months
           (val >= 0x3400 && val <= 0x4DBF) || // CJK Unified Ideographs Extension A
           (val >= 0x4E00 && val <= 0x9FFF) || // CJK Unified Ideographs
           (val >= 0xF900 && val <= 0xFAFF) || // CJK Compatibility Ideographs
           (val >= 0xFF00 && val <= 0xFFEF) || // Halfwidth and Fullwidth Forms
           (val >= 0xAC00 && val <= 0xD7AF) || // Hangul Syllables
           (val >= 0x20000 && val <= 0x3FFFD)   // Plane 2 & 3 CJK Ideographs
        {
            2
        } else {
            1
        }
    }
}

fn str_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

fn split_list_line(line: &str) -> Option<(String, String, String)> {
    let indent = line
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();
    let trimmed = line.trim_start();
    
    if !is_list_item(trimmed) {
        return None;
    }

    if trimmed.starts_with('-') || trimmed.starts_with('+') || trimmed.starts_with('*') {
        let bullet_char = trimmed.chars().next().unwrap();
        let after_bullet = &trimmed[1..];
        let spaces = after_bullet
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();
        let bullet_part = if spaces.is_empty() {
            format!("{}", bullet_char)
        } else {
            format!("{}{}", bullet_char, spaces)
        };
        let content = after_bullet[spaces.len()..].trim().to_string();
        return Some((indent, bullet_part, content));
    }

    if let Some(first_char) = trimmed.chars().next() {
        if first_char.is_ascii_digit() {
            let digits_count = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
            let after_digits = &trimmed[digits_count..];
            if after_digits.starts_with('.') || after_digits.starts_with(')') {
                let bullet_marker = &trimmed[..digits_count + 1];
                let after_marker = &trimmed[digits_count + 1..];
                let spaces = after_marker
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .collect::<String>();
                let bullet_part = format!("{}{}", bullet_marker, spaces);
                let content = after_marker[spaces.len()..].trim().to_string();
                return Some((indent, bullet_part, content));
            }
        } else if first_char.is_ascii_lowercase() {
            let after_char = &trimmed[1..];
            if after_char.starts_with('.') || after_char.starts_with(')') {
                let bullet_marker = &trimmed[..2];
                let after_marker = &trimmed[2..];
                let spaces = after_marker
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .collect::<String>();
                let bullet_part = format!("{}{}", bullet_marker, spaces);
                let content = after_marker[spaces.len()..].trim().to_string();
                return Some((indent, bullet_part, content));
            }
        }
    }

    None
}

/// Reflow a list item (its bullet line followed by any continuation lines) to the fill column,
/// aligning wrapped lines with the text after the bullet.
fn wrap_list_item(item: &[String], fill_column: usize) -> Vec<String> {
    let line = &item[0];
    if let Some((indent, bullet, first_content)) = split_list_line(line) {
        let content = std::iter::once(first_content.as_str())
            .chain(item[1..].iter().map(|l| l.trim()))
            .collect::<Vec<_>>()
            .join(" ");
        let words: Vec<&str> = content.split_whitespace().collect();
        if words.is_empty() {
            return vec![format_list_line(line)];
        }

        let bullet = format!("{} ", bullet.trim_end());
        let indent_width = str_width(&indent);
        let bullet_width = str_width(&bullet);
        let first_line_prefix_width = indent_width + bullet_width;

        let subsequent_indent = format!("{}{}", indent, " ".repeat(bullet_width));
        let subsequent_indent_width = indent_width + bullet_width;

        let mut wrapped_lines = Vec::new();
        let mut current_line = String::new();

        let first_target_width = if fill_column > first_line_prefix_width {
            fill_column - first_line_prefix_width
        } else {
            1
        };

        let subsequent_target_width = if fill_column > subsequent_indent_width {
            fill_column - subsequent_indent_width
        } else {
            1
        };

        let mut is_first_line = true;

        for word in words {
            let word_width = str_width(word);
            let current_width = str_width(&current_line);
            let target_width = if is_first_line { first_target_width } else { subsequent_target_width };

            if current_line.is_empty() {
                current_line.push_str(word);
            } else if current_width + 1 + word_width <= target_width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                if is_first_line {
                    wrapped_lines.push(format!("{}{}{}", indent, bullet, current_line));
                    is_first_line = false;
                } else {
                    wrapped_lines.push(format!("{}{}", subsequent_indent, current_line));
                }
                current_line = word.to_string();
            }
        }

        if !current_line.is_empty() {
            if is_first_line {
                wrapped_lines.push(format!("{}{}{}", indent, bullet, current_line));
            } else {
                wrapped_lines.push(format!("{}{}", subsequent_indent, current_line));
            }
        }

        wrapped_lines
    } else {
        item.to_vec()
    }
}

fn wrap_paragraph(paragraph: &[String], fill_column: usize) -> Vec<String> {
    if paragraph.is_empty() {
        return Vec::new();
    }

    let first_line = &paragraph[0];
    let indent = first_line
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();
    let indent_len = indent.len();

    // Clean the text by stripping the common indentation
    let cleaned_lines: Vec<String> = paragraph
        .iter()
        .map(|line| {
            if line.starts_with(&indent) {
                line[indent_len..].to_string()
            } else {
                line.trim_start().to_string()
            }
        })
        .collect();

    let combined_text = cleaned_lines.join(" ");
    let words: Vec<&str> = combined_text.split_whitespace().collect();

    if words.is_empty() {
        return Vec::new();
    }

    let mut wrapped_lines = Vec::new();
    let mut current_line = String::new();

    let indent_width = str_width(&indent);
    // Ensure target column accounts for indentation
    let target_width = if fill_column > indent_width {
        fill_column - indent_width
    } else {
        1 // Fallback in case indentation is larger than fill_column
    };

    for word in words {
        let word_width = str_width(word);
        let current_width = str_width(&current_line);
        if current_line.is_empty() {
            current_line.push_str(word);
        } else if current_width + 1 + word_width <= target_width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            wrapped_lines.push(format!("{}{}", indent, current_line));
            current_line = word.to_string();
        }
    }

    if !current_line.is_empty() {
        wrapped_lines.push(format!("{}{}", indent, current_line));
    }

    wrapped_lines
}

fn block_closing_tag(trimmed_upper: &str) -> String {
    let first_word_upper = trimmed_upper.split_whitespace().next().unwrap_or("");
    let keyword = first_word_upper.strip_prefix("#+BEGIN_").unwrap_or("");
    format!("#+END_{}", keyword)
}

fn is_drawer_start(trimmed_upper: &str) -> bool {
    trimmed_upper.starts_with(':') && trimmed_upper.ends_with(':') && trimmed_upper != ":END:"
}

fn leading_whitespace(line: &str) -> &str {
    let len = line.len() - line.trim_start().len();
    &line[..len]
}

/// Keyword lines (e.g. #+TITLE) before the first heading stay at column 0; all other
/// non-heading, non-drawer content is left-indented.
fn should_left_indent(line: &str, seen_heading: bool) -> bool {
    seen_heading || !line.trim_start().starts_with("#+")
}

/// Replace the section's base indentation (`base` columns) with a single tab, keeping any
/// indentation beyond the base as spaces so nested lists and continuation lines are preserved.
fn left_indent_prefix(line: &str, base: usize) -> String {
    let width = str_width(leading_whitespace(line));
    format!("\t{}", " ".repeat(width.saturating_sub(base)))
}

/// For each line, the minimum indentation width among the left-indentable lines of its
/// section, where sections are the preamble and the span under each heading.
fn section_base_indents(lines: &[&str]) -> Vec<usize> {
    let mut bases = vec![0; lines.len()];
    let mut section_start = 0;
    let mut section_min: Option<usize> = None;
    let mut seen_heading = false;
    let mut closing_tag: Option<String> = None;
    let mut in_drawer = false;
    let mut after_heading = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed_upper = line.trim().to_uppercase();
        if !trimmed_upper.is_empty() {
            let was_after_heading = after_heading;
            after_heading = false;
            // Planning lines keep their indentation, so they don't count toward the base
            if was_after_heading && closing_tag.is_none() && !in_drawer && is_planning_line(line) {
                continue;
            }
        }
        if let Some(tag) = &closing_tag {
            if trimmed_upper.starts_with(tag.as_str()) {
                closing_tag = None;
            }
            continue;
        }
        if in_drawer {
            if trimmed_upper == ":END:" {
                in_drawer = false;
            }
            continue;
        }
        if trimmed_upper.is_empty() {
            continue;
        }

        if trimmed_upper.starts_with("#+BEGIN_") {
            closing_tag = Some(block_closing_tag(&trimmed_upper));
        } else if is_drawer_start(&trimmed_upper) {
            in_drawer = true;
            continue;
        } else if is_heading_line(line) {
            for base in &mut bases[section_start..i] {
                *base = section_min.unwrap_or(0);
            }
            section_start = i;
            section_min = None;
            seen_heading = true;
            after_heading = true;
            continue;
        }

        if should_left_indent(line, seen_heading) {
            let width = str_width(leading_whitespace(line));
            section_min = Some(section_min.map_or(width, |m| m.min(width)));
        }
    }

    for base in &mut bases[section_start..] {
        *base = section_min.unwrap_or(0);
    }
    bases
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ElementKind {
    Paragraph,
    List,
    Table,
    TableFormula,
    Block,
    FixedWidth,
    Comment,
    Keyword,
    AffiliatedKeyword,
    Drawer,
    HorizontalRule,
}

/// Keywords that attach to the element directly below them (e.g. #+CAPTION above a table).
fn is_affiliated_keyword(trimmed: &str) -> bool {
    let name: String = trimmed
        .trim_start_matches("#+")
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect::<String>()
        .to_uppercase();
    name.starts_with("ATTR_")
        || matches!(
            name.as_str(),
            "CAPTION" | "NAME" | "HEADER" | "HEADERS" | "PLOT" | "RESULTS" | "TBLNAME" | "DATA"
                | "LABEL" | "SOURCE" | "SRCNAME"
        )
}

fn element_kind(line: &str) -> ElementKind {
    let trimmed = line.trim();
    let upper = trimmed.to_uppercase();
    if trimmed.starts_with('|') {
        ElementKind::Table
    } else if upper.starts_with("#+BEGIN_") {
        ElementKind::Block
    } else if upper.starts_with("#+TBLFM:") {
        ElementKind::TableFormula
    } else if trimmed.starts_with("#+") {
        if is_affiliated_keyword(trimmed) {
            ElementKind::AffiliatedKeyword
        } else {
            ElementKind::Keyword
        }
    } else if trimmed == "#" || trimmed.starts_with("# ") {
        ElementKind::Comment
    } else if trimmed == ":" || trimmed.starts_with(": ") {
        ElementKind::FixedWidth
    } else if is_drawer_start(&upper) {
        ElementKind::Drawer
    } else if trimmed.len() >= 5 && trimmed.chars().all(|c| c == '-') {
        ElementKind::HorizontalRule
    } else {
        ElementKind::Paragraph
    }
}

/// Like `element_kind`, but plain text indented past the current list's outermost bullet counts
/// as part of the list. `list_base` tracks that bullet's indentation width while inside a list.
fn classify_element(line: &str, indent_width: usize, list_base: &mut Option<usize>) -> ElementKind {
    if is_list_item(line) {
        if list_base.map_or(true, |base| indent_width < base) {
            *list_base = Some(indent_width);
        }
        return ElementKind::List;
    }
    if let Some(base) = *list_base {
        if indent_width <= base {
            *list_base = None;
        } else if is_regular_paragraph_line(line) {
            return ElementKind::List;
        }
    }
    element_kind(line)
}

/// Blank lines to emit between consecutive body elements given the blank lines already pending.
/// Affiliated keywords always hug the element below them, comments stick to whatever follows,
/// and #+TBLFM stays attached to its table.
fn body_spacing(
    prev: Option<ElementKind>,
    next: ElementKind,
    pending: usize,
    spacing: usize,
) -> usize {
    use ElementKind::*;
    match prev {
        None => pending,
        Some(AffiliatedKeyword) => 0,
        Some(prev) => {
            let attached =
                prev == next || prev == Comment || (prev == Table && next == TableFormula);
            if attached { pending } else { pending.max(spacing) }
        }
    }
}

fn format_org(input: &str, options: &FormatOptions) -> String {
    let mut lines: Vec<&str> = input.lines().collect();

    // 1. Trim leading and trailing empty lines from the document boundaries
    while !lines.is_empty() && lines[0].trim().is_empty() {
        lines.remove(0);
    }
    while !lines.is_empty() && lines[lines.len() - 1].trim().is_empty() {
        lines.pop();
    }

    if lines.is_empty() {
        return String::new();
    }

    let mut formatted_lines = Vec::new();
    let mut state = State::Normal;
    let mut pending_empty_lines = 0;
    let mut is_first_line = true;
    let mut seen_heading = false;
    // Kind of the previous body element, or None right after a heading/drawer/file start
    let mut last_kind: Option<ElementKind> = None;
    let mut list_base: Option<usize> = None;
    let base_indents = if options.left_indent {
        section_base_indents(&lines)
    } else {
        Vec::new()
    };

    // Consecutive paragraph lines accumulator
    let mut accumulated_paragraph: Vec<String> = Vec::new();
    // Current list item's bullet line plus continuation lines (only used with wrap_lists).
    // Spacing before the item is emitted when the item starts, so flushing only adds its lines.
    let mut accumulated_list_item: Vec<String> = Vec::new();

    let flush_paragraph = |accumulated: &mut Vec<String>,
                           list_item: &mut Vec<String>,
                           formatted: &mut Vec<String>,
                           pending_lines: &mut usize,
                           is_first: &mut bool| {
        if !list_item.is_empty() {
            formatted.extend(wrap_list_item(list_item, options.fill_column));
            list_item.clear();
        }

        if accumulated.is_empty() {
            return;
        }

        if !*is_first {
            for _ in 0..*pending_lines {
                formatted.push(String::new());
            }
        }
        *pending_lines = 0;

        let wrapped = wrap_paragraph(accumulated, options.fill_column);
        for line in wrapped {
            formatted.push(line);
        }

        accumulated.clear();
        *is_first = false;
    };

    for (idx, raw_line) in lines.into_iter().enumerate() {
        let trimmed = raw_line.trim_end();
        let is_empty = trimmed.trim().is_empty();

        // Indentation for non-heading, non-drawer content lines
        let content_indent = if options.left_indent && should_left_indent(raw_line, seen_heading) {
            left_indent_prefix(raw_line, base_indents[idx])
        } else {
            leading_whitespace(raw_line).to_string()
        };
        let content = format!("{}{}", content_indent, trimmed.trim_start());

        match state {
            State::InBlock(ref closing_tag) => {
                let trimmed_upper = trimmed.trim().to_uppercase();
                if trimmed_upper.starts_with(closing_tag) {
                    let indent = content_indent;
                    formatted_lines.push(format!("{}{}", indent, closing_tag));
                    state = State::Normal;
                    pending_empty_lines = 1;
                } else {
                    formatted_lines.push(trimmed.to_string());
                }
            }
            State::InDrawer { ref indent } => {
                let trimmed_upper = trimmed.trim().to_uppercase();
                if trimmed_upper == ":END:" {
                    formatted_lines.push(format!("{}{}", indent, ":END:"));
                    state = State::Normal;
                    pending_empty_lines = options.heading_to_body_spacing;
                    last_kind = None;
                    list_base = None;
                } else if is_empty {
                    // Collapse empty lines inside drawers
                } else {
                    let inner_trimmed = trimmed.trim_start();
                    if inner_trimmed.starts_with(':') {
                        if let Some(colon_idx) = inner_trimmed[1..].find(':') {
                            let prop_name = &inner_trimmed[..colon_idx + 2];
                            let prop_value = inner_trimmed[colon_idx + 2..].trim();
                            if prop_value.is_empty() {
                                formatted_lines.push(format!("{}{}", indent, prop_name));
                            } else {
                                formatted_lines
                                    .push(format!("{}{} {}", indent, prop_name, prop_value));
                            }
                        } else {
                            formatted_lines.push(format!("{}{}", indent, inner_trimmed));
                        }
                    } else {
                        formatted_lines.push(format!("{}{}", indent, inner_trimmed));
                    }
                }
            }
            State::Normal => {
                if is_empty {
                    flush_paragraph(
                        &mut accumulated_paragraph,
                        &mut accumulated_list_item,
                        &mut formatted_lines,
                        &mut pending_empty_lines,
                        &mut is_first_line,
                    );
                    pending_empty_lines = 1;
                    continue;
                }

                let trimmed_upper = trimmed.trim().to_uppercase();
                let kind = classify_element(trimmed, str_width(&content_indent), &mut list_base);
                let apply_body_spacing = |pending: &mut usize, last: &mut Option<ElementKind>| {
                    if options.body_to_body_spacing > 0 {
                        *pending = body_spacing(*last, kind, *pending, options.body_to_body_spacing);
                    }
                    *last = Some(kind);
                };

                if trimmed_upper.starts_with("#+BEGIN_") {
                    flush_paragraph(
                        &mut accumulated_paragraph,
                        &mut accumulated_list_item,
                        &mut formatted_lines,
                        &mut pending_empty_lines,
                        &mut is_first_line,
                    );

                    let last_is_header = formatted_lines
                        .last()
                        .map(|l| is_heading_or_drawer_end(l))
                        .unwrap_or(false);
                    if last_is_header {
                        pending_empty_lines = options.heading_to_body_spacing;
                    }
                    apply_body_spacing(&mut pending_empty_lines, &mut last_kind);

                    if !is_first_line {
                        for _ in 0..pending_empty_lines {
                            formatted_lines.push(String::new());
                        }
                    }
                    pending_empty_lines = 0;

                    let closing_tag = block_closing_tag(&trimmed_upper);

                    let indent = content_indent;
                    let block_line = trimmed.trim_start();
                    let first_word = block_line.split_whitespace().next().unwrap_or("");
                    let type_rest = block_line.strip_prefix(first_word).unwrap_or("").trim();
                    if type_rest.is_empty() {
                        formatted_lines.push(format!("{}{}", indent, first_word.to_uppercase()));
                    } else {
                        let mut words = type_rest.split_whitespace();
                        if let Some(block_type) = words.next() {
                            let block_type_lower = block_type.to_lowercase();
                            let rest = type_rest.strip_prefix(block_type).unwrap_or("").trim();
                            if rest.is_empty() {
                                formatted_lines.push(format!(
                                    "{}{}{} {}",
                                    indent,
                                    first_word.to_uppercase(),
                                    "",
                                    block_type_lower
                                ));
                            } else {
                                formatted_lines.push(format!(
                                    "{}{}{} {} {}",
                                    indent,
                                    first_word.to_uppercase(),
                                    "",
                                    block_type_lower,
                                    rest
                                ));
                            }
                        } else {
                            formatted_lines.push(format!(
                                "{}{}{} {}",
                                indent,
                                first_word.to_uppercase(),
                                "",
                                type_rest
                            ));
                        }
                    }
                    state = State::InBlock(closing_tag);
                    is_first_line = false;
                } else if is_drawer_start(&trimmed_upper) {
                    flush_paragraph(
                        &mut accumulated_paragraph,
                        &mut accumulated_list_item,
                        &mut formatted_lines,
                        &mut pending_empty_lines,
                        &mut is_first_line,
                    );

                    let last_is_header = formatted_lines
                        .last()
                        .map(|l| is_heading_or_drawer_end(l))
                        .unwrap_or(false);

                    let mut indent = raw_line
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .collect::<String>();

                    if last_is_header {
                        pending_empty_lines = 0;
                        if !indent.is_empty() {
                            if let Some(last_line) = formatted_lines.last() {
                                if let Some(level) = get_heading_level(last_line) {
                                    indent = " ".repeat(level + 1);
                                }
                            }
                        }
                    } else if !is_first_line {
                        apply_body_spacing(&mut pending_empty_lines, &mut last_kind);
                        for _ in 0..pending_empty_lines {
                            formatted_lines.push(String::new());
                        }
                        pending_empty_lines = 0;
                    }

                    formatted_lines.push(format!("{}{}", indent, trimmed_upper));
                    state = State::InDrawer { indent };
                    is_first_line = false;
                } else if is_heading_line(raw_line) {
                    flush_paragraph(
                        &mut accumulated_paragraph,
                        &mut accumulated_list_item,
                        &mut formatted_lines,
                        &mut pending_empty_lines,
                        &mut is_first_line,
                    );

                    let last_is_header = formatted_lines
                        .last()
                        .map(|l| is_heading_or_drawer_end(l))
                        .unwrap_or(false);

                    let spacing = if last_is_header {
                        options.heading_to_heading_spacing
                    } else {
                        if is_first_line { 0 } else { 1 }
                    };

                    for _ in 0..spacing {
                        formatted_lines.push(String::new());
                    }
                    pending_empty_lines = 0;
                    formatted_lines.push(trimmed.trim_start().to_string());
                    is_first_line = false;
                    seen_heading = true;
                    last_kind = None;
                    list_base = None;
                } else if is_planning_line(trimmed)
                    && formatted_lines.last().map_or(false, |l| is_heading_line(l))
                {
                    // Planning lines must directly follow their heading to be recognized by Org
                    pending_empty_lines = 0;
                    formatted_lines.push(trimmed.to_string());
                } else if is_list_item(trimmed) {
                    flush_paragraph(
                        &mut accumulated_paragraph,
                        &mut accumulated_list_item,
                        &mut formatted_lines,
                        &mut pending_empty_lines,
                        &mut is_first_line,
                    );

                    let last_is_header = formatted_lines
                        .last()
                        .map(|l| is_heading_or_drawer_end(l))
                        .unwrap_or(false);
                    if last_is_header {
                        pending_empty_lines = options.heading_to_body_spacing;
                    }
                    apply_body_spacing(&mut pending_empty_lines, &mut last_kind);

                    if !is_first_line {
                        for _ in 0..pending_empty_lines {
                            formatted_lines.push(String::new());
                        }
                    }
                    pending_empty_lines = 0;

                    if options.wrap_lists {
                        accumulated_list_item.push(content);
                    } else {
                        formatted_lines.push(format_list_line(&content));
                    }
                    is_first_line = false;
                } else {
                    let is_list_continuation = !accumulated_list_item.is_empty()
                        && is_regular_paragraph_line(raw_line)
                        && str_width(&content_indent)
                            > str_width(leading_whitespace(&accumulated_list_item[0]));

                    if is_list_continuation {
                        accumulated_list_item.push(content);
                    } else if options.wrap_paragraphs && is_regular_paragraph_line(raw_line) {
                        let is_dedented = accumulated_paragraph.first().map_or(false, |first| {
                            str_width(&content_indent) < str_width(leading_whitespace(first))
                        });
                        if !accumulated_list_item.is_empty() || is_dedented {
                            flush_paragraph(
                                &mut accumulated_paragraph,
                                &mut accumulated_list_item,
                                &mut formatted_lines,
                                &mut pending_empty_lines,
                                &mut is_first_line,
                            );
                        }
                        if accumulated_paragraph.is_empty() {
                            let last_is_header = formatted_lines
                                .last()
                                .map(|l| is_heading_or_drawer_end(l))
                                .unwrap_or(false);
                            if last_is_header {
                                pending_empty_lines = options.heading_to_body_spacing;
                            }
                            apply_body_spacing(&mut pending_empty_lines, &mut last_kind);
                        }
                        accumulated_paragraph.push(content);
                    } else {
                        flush_paragraph(
                            &mut accumulated_paragraph,
                            &mut accumulated_list_item,
                            &mut formatted_lines,
                            &mut pending_empty_lines,
                            &mut is_first_line,
                        );

                        let last_is_header = formatted_lines
                            .last()
                            .map(|l| is_heading_or_drawer_end(l))
                            .unwrap_or(false);
                        if last_is_header {
                            pending_empty_lines = options.heading_to_body_spacing;
                        }
                        apply_body_spacing(&mut pending_empty_lines, &mut last_kind);

                        if !is_first_line {
                            for _ in 0..pending_empty_lines {
                                formatted_lines.push(String::new());
                            }
                        }
                        pending_empty_lines = 0;
                        formatted_lines.push(content);
                        is_first_line = false;
                    }
                }
            }
        }
    }

    // Flush any remaining accumulated paragraph at the end of the file
    flush_paragraph(
        &mut accumulated_paragraph,
        &mut accumulated_list_item,
        &mut formatted_lines,
        &mut pending_empty_lines,
        &mut is_first_line,
    );

    formatted_lines.join("\n") + "\n"
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let check_mode = args.contains(&"--check".to_string());

    let mut options = FormatOptions::default();
    for arg in &args {
        if arg.starts_with("--heading-to-body-spacing=") {
            if let Some(val_str) = arg.split('=').nth(1) {
                if let Ok(val) = val_str.parse::<usize>() {
                    options.heading_to_body_spacing = val;
                }
            }
        }
        if arg.starts_with("--heading-to-heading-spacing=") {
            if let Some(val_str) = arg.split('=').nth(1) {
                if let Ok(val) = val_str.parse::<usize>() {
                    options.heading_to_heading_spacing = val;
                }
            }
        }
        if arg == "--wrap-paragraphs" {
            options.wrap_paragraphs = true;
        }
        if arg == "--wrap-lists" {
            options.wrap_lists = true;
        }
        if arg == "--left-indent" {
            options.left_indent = true;
        }
        if arg.starts_with("--body-to-body-spacing=") {
            if let Some(val_str) = arg.split('=').nth(1) {
                if let Ok(val) = val_str.parse::<usize>() {
                    options.body_to_body_spacing = val;
                }
            }
        }
        if arg.starts_with("--fill-column=") {
            if let Some(val_str) = arg.split('=').nth(1) {
                if let Ok(val) = val_str.parse::<usize>() {
                    options.fill_column = val;
                }
            }
        }
    }

    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("Error reading stdin: {}", e);
        std::process::exit(1);
    }

    let formatted = format_org(&input, &options);

    if check_mode {
        if formatted != input {
            eprintln!("File is not formatted!");
            std::process::exit(1);
        } else {
            println!("File is formatted!");
            std::process::exit(0);
        }
    } else {
        print!("{}", formatted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heading_spacing() {
        let input = "
* Heading 1

Some text.


** Heading 2
:PROPERTIES:
:CUSTOM_ID: 123
:END:
More text.
";
        let expected = "* Heading 1

Some text.

** Heading 2
:PROPERTIES:
:CUSTOM_ID: 123
:END:

More text.
";
        assert_eq!(format_org(input, &FormatOptions::default()), expected);
    }

    #[test]
    fn test_list_items() {
        let input = "
-    Item 1 with lots of spaces
- Item 2
+ Item 3
  1.    Ordered list item
";
        let expected = "- Item 1 with lots of spaces
- Item 2
+ Item 3
  1. Ordered list item
";
        assert_eq!(format_org(input, &FormatOptions::default()), expected);
    }

    #[test]
    fn test_block_preservation() {
        let input = "
#+begin_src rust
fn main() {
    println!(\"Hello World\");


    // consecutive spaces are kept here!
}
#+end_src
";
        let expected = "#+BEGIN_SRC rust
fn main() {
    println!(\"Hello World\");


    // consecutive spaces are kept here!
}
#+END_SRC
";
        assert_eq!(format_org(input, &FormatOptions::default()), expected);
    }

    #[test]
    fn test_enhanced_drawer_formatting() {
        let input = "* Heading 1
  :PROPERTIES:
  :CUSTOM_ID:     456
  :ANOTHER_PROP:  abc
  :END:
Some text.";
        let expected = "* Heading 1
  :PROPERTIES:
  :CUSTOM_ID: 456
  :ANOTHER_PROP: abc
  :END:

Some text.
";
        assert_eq!(format_org(input, &FormatOptions::default()), expected);
    }

    #[test]
    fn test_heading_leading_spaces() {
        let input = "   ** Subheading 1.1
More text.";
        let expected = "** Subheading 1.1

More text.
";
        assert_eq!(format_org(input, &FormatOptions::default()), expected);
    }

    #[test]
    fn test_heading_followed_by_text_no_newline() {
        let input = "* Heading 1
Some body text.";
        let expected = "* Heading 1

Some body text.
";
        assert_eq!(format_org(input, &FormatOptions::default()), expected);
    }

    #[test]
    fn test_block_type_lowercase() {
        let input = "#+BEGIN_SRC RUST :tangle yes
fn main() {}
#+END_SRC";
        let expected = "#+BEGIN_SRC rust :tangle yes
fn main() {}
#+END_SRC
";
        assert_eq!(format_org(input, &FormatOptions::default()), expected);
    }

    #[test]
    fn test_list_items_alphabetical_and_stars() {
        let input = "
  - item 1
  * item 2
  a.   alphabetical 1
  b)   alphabetical 2
";
        let expected = "  - item 1
  * item 2
  a. alphabetical 1
  b) alphabetical 2
";
        assert_eq!(format_org(input, &FormatOptions::default()), expected);
    }

    #[test]
    fn test_custom_heading_to_body_spacing_0() {
        let input = "* Heading 1

Some text.
** Heading 2
Some text.";
        let options = FormatOptions {
            heading_to_body_spacing: 0,
            heading_to_heading_spacing: 1,
            ..FormatOptions::default()
        };
        let expected = "* Heading 1
Some text.

** Heading 2
Some text.
";
        assert_eq!(format_org(input, &options), expected);
    }

    #[test]
    fn test_custom_heading_to_heading_spacing_0() {
        let input = "* Heading 1

** Heading 2
Some text.";
        let options = FormatOptions {
            heading_to_body_spacing: 1,
            heading_to_heading_spacing: 0,
            ..FormatOptions::default()
        };
        let expected = "* Heading 1
** Heading 2

Some text.
";
        assert_eq!(format_org(input, &options), expected);
    }

    #[test]
    fn test_wrap_paragraphs_disabled_by_default() {
        let input = "This is a long line that should not be wrapped because paragraph wrapping is disabled by default.";
        let expected = "This is a long line that should not be wrapped because paragraph wrapping is disabled by default.
";
        assert_eq!(format_org(input, &FormatOptions::default()), expected);
    }

    #[test]
    fn test_wrap_paragraphs_enabled() {
        let input = "This is a long paragraph that should be wrapped to at most forty characters width. It has multiple words.";
        let options = FormatOptions {
            wrap_paragraphs: true,
            fill_column: 40,
            ..FormatOptions::default()
        };
        let expected = "This is a long paragraph that should be
wrapped to at most forty characters
width. It has multiple words.
";
        assert_eq!(format_org(input, &options), expected);
    }

    #[test]
    fn test_wrap_indented_paragraphs() {
        let input = "  This is an indented paragraph that has
  multiple lines and should preserve
  the two spaces indentation.";
        let options = FormatOptions {
            wrap_paragraphs: true,
            fill_column: 50,
            ..FormatOptions::default()
        };
        let expected = "  This is an indented paragraph that has multiple
  lines and should preserve the two spaces
  indentation.
";
        assert_eq!(format_org(input, &options), expected);
    }

    #[test]
    fn test_custom_heading_to_body_spacing_2() {
        let input = "* Heading 1
Some body text.
** Heading 2
Some other body text.";
        let options = FormatOptions {
            heading_to_body_spacing: 2,
            ..FormatOptions::default()
        };
        let expected = "* Heading 1


Some body text.

** Heading 2


Some other body text.
";
        assert_eq!(format_org(input, &options), expected);
    }

    #[test]
    fn test_custom_heading_to_body_spacing_with_drawer_and_wrap() {
        let input = "* Heading 1
  :PROPERTIES:
  :CUSTOM_ID: abc
  :END:
This is some body text that will be wrapped because wrap paragraphs is enabled and it is quite long.
* Heading 2
This is some other body text under heading 2.";
        let options = FormatOptions {
            heading_to_body_spacing: 2,
            wrap_paragraphs: true,
            fill_column: 40,
            ..FormatOptions::default()
        };
        let expected = "* Heading 1
  :PROPERTIES:
  :CUSTOM_ID: abc
  :END:


This is some body text that will be
wrapped because wrap paragraphs is
enabled and it is quite long.

* Heading 2


This is some other body text under
heading 2.
";
        assert_eq!(format_org(input, &options), expected);
    }

    #[test]
    fn test_wrap_paragraphs_korean() {
        let input = "오알지포매터는 이맥스의 기능을 러스트로 구현하여 빠르게 문서를 포맷팅할 수 있는 아주 유용한 도구입니다.";
        let options = FormatOptions {
            wrap_paragraphs: true,
            fill_column: 40,
            ..FormatOptions::default()
        };
        let expected = "오알지포매터는 이맥스의 기능을 러스트로
구현하여 빠르게 문서를 포맷팅할 수 있는
아주 유용한 도구입니다.
";
        assert_eq!(format_org(input, &options), expected);
    }

    #[test]
    fn test_wrap_lists() {
        let input = "  - This is a very long list item line that should be nicely wrapped to multiple lines preserving the bullet indentation.";
        let options = FormatOptions {
            wrap_lists: true,
            fill_column: 40,
            ..FormatOptions::default()
        };
        let expected = "  - This is a very long list item line
    that should be nicely wrapped to
    multiple lines preserving the bullet
    indentation.
";
        assert_eq!(format_org(input, &options), expected);
    }

    #[test]
    fn test_wrap_lists_korean() {
        let input = "  * 매우 긴 한글 리스트 아이템입니다. 이것은 지정된 너비에 맞추어 여러 줄로 깔끔하게 래핑되어야 합니다.";
        let options = FormatOptions {
            wrap_lists: true,
            fill_column: 40,
            ..FormatOptions::default()
        };
        let expected = "  * 매우 긴 한글 리스트 아이템입니다.
    이것은 지정된 너비에 맞추어 여러
    줄로 깔끔하게 래핑되어야 합니다.
";
        assert_eq!(format_org(input, &options), expected);
    }

    #[test]
    fn test_wrap_lists_ordered() {
        let input = " 1. First ordered item that spans a considerable length to test the ordered list wrapping.";
        let options = FormatOptions {
            wrap_lists: true,
            fill_column: 40,
            ..FormatOptions::default()
        };
        let expected = " 1. First ordered item that spans a
    considerable length to test the
    ordered list wrapping.
";
        assert_eq!(format_org(input, &options), expected);
    }

    #[test]
    fn test_indented_block_keeps_header_args() {
        let input = "  #+begin_src python :results output
x
  #+end_src";
        let expected = "  #+BEGIN_SRC python :results output
x
  #+END_SRC
";
        assert_eq!(format_org(input, &FormatOptions::default()), expected);
    }

    #[test]
    fn test_planning_line_stays_under_heading() {
        let input = "* TODO Task

SCHEDULED: <2026-11-19 Thu> DEADLINE: <2026-11-20 Fri>
:PROPERTIES:
:ID: x
:END:
:LOGBOOK:
CLOCK: [2026-11-19 Thu 09:00]--[2026-11-19 Thu 10:00] =>  1:00
:END:
Body text.";
        let expected = "* TODO Task
SCHEDULED: <2026-11-19 Thu> DEADLINE: <2026-11-20 Fri>
:PROPERTIES:
:ID: x
:END:
:LOGBOOK:
CLOCK: [2026-11-19 Thu 09:00]--[2026-11-19 Thu 10:00] =>  1:00
:END:

\tBody text.
";
        let options = left_indent_options();
        let formatted = format_org(input, &options);
        assert_eq!(formatted, expected);
        assert_eq!(format_org(&formatted, &options), formatted);
        assert_eq!(
            format_org("* H\nSCHEDULED: <2026-11-19 Thu>\nText.", &FormatOptions::default()),
            "* H\nSCHEDULED: <2026-11-19 Thu>\n\nText.\n"
        );
    }

    fn body_spacing_options(spacing: usize) -> FormatOptions {
        FormatOptions {
            body_to_body_spacing: spacing,
            ..FormatOptions::default()
        }
    }

    #[test]
    fn test_body_spacing_between_element_types() {
        let input = "#+TITLE: T
#+STARTUP: overview
Intro.
* H
Text.
- item
  continuation
- item 2
| a |
Text.";
        let expected = "#+TITLE: T
#+STARTUP: overview

Intro.

* H

Text.

- item
  continuation
- item 2

| a |

Text.
";
        assert_eq!(format_org(input, &body_spacing_options(1)), expected);
    }

    #[test]
    fn test_body_spacing_attached_elements() {
        let input = "Text.
#+CAPTION: cap

#+NAME: tbl
| a |
#+TBLFM: $1=1
# comment
#+begin_src sh
echo
#+end_src
#+RESULTS:

: echo";
        let expected = "Text.

#+CAPTION: cap
#+NAME: tbl
| a |
#+TBLFM: $1=1

# comment
#+BEGIN_SRC sh
echo
#+END_SRC

#+RESULTS:
: echo
";
        assert_eq!(format_org(input, &body_spacing_options(1)), expected);
    }

    #[test]
    fn test_body_spacing_inside_list_items() {
        let input = "- item
  | nested | table |
  more item text
  - nested item
  nested continuation
Text.";
        let expected = "- item

  | nested | table |

  more item text
  - nested item
  nested continuation

Text.
";
        assert_eq!(format_org(input, &body_spacing_options(1)), expected);
    }

    #[test]
    fn test_body_spacing_multiple_lines_and_existing_blanks_kept() {
        let input = "* H
Text.
- item

Text.";
        let expected = "* H

Text.


- item


Text.
";
        assert_eq!(format_org(input, &body_spacing_options(2)), expected);
        assert_eq!(
            format_org("Text.\n\n- item", &body_spacing_options(0)),
            "Text.\n\n- item\n"
        );
        assert_eq!(format_org("Text.\n- item", &body_spacing_options(0)), "Text.\n- item\n");
    }

    #[test]
    fn test_body_spacing_idempotent_with_all_options() {
        let input = "#+TITLE: T
Intro.
* H
SCHEDULED: <2026-11-19 Thu>
:PROPERTIES:
:ID: x
:END:
Text that is long enough to wrap when the fill column is small.
- item that is long enough to wrap when the fill column is small
  | t |
#+CAPTION: c
| a |
: fixed
-----
Text.";
        let options = FormatOptions {
            body_to_body_spacing: 1,
            left_indent: true,
            wrap_paragraphs: true,
            wrap_lists: true,
            fill_column: 40,
            ..FormatOptions::default()
        };
        let once = format_org(input, &options);
        assert_eq!(format_org(&once, &options), once);
    }

    #[test]
    fn test_wrap_paragraphs_skips_fixed_width_and_rules() {
        let input = "Text.
: fixed width
-----
More text.";
        let options = FormatOptions {
            wrap_paragraphs: true,
            ..FormatOptions::default()
        };
        assert_eq!(format_org(input, &options), format!("{}\n", input));
    }

    fn wrap_lists_options(fill_column: usize) -> FormatOptions {
        FormatOptions {
            wrap_lists: true,
            fill_column,
            ..FormatOptions::default()
        }
    }

    #[test]
    fn test_wrap_lists_reflows_continuation_lines() {
        let input = "-   First item that was
  wrapped too
  early by hand and now needs reflowing.
- Second item.
Not a continuation.";
        let expected = "- First item that was wrapped too early by
  hand and now needs reflowing.
- Second item.
Not a continuation.
";
        assert_eq!(format_org(input, &wrap_lists_options(45)), expected);
    }

    #[test]
    fn test_wrap_lists_nested_and_numbered() {
        let input = "- Parent item with enough text to need wrapping here.
  - Child item with enough text to need wrapping as well.
10. Numbered item with enough text to need wrapping here.";
        let expected = "- Parent item with enough text to need
  wrapping here.
  - Child item with enough text to need
    wrapping as well.
10. Numbered item with enough text to need
    wrapping here.
";
        assert_eq!(format_org(input, &wrap_lists_options(42)), expected);
    }

    #[test]
    fn test_wrap_lists_checkbox_aligns_after_bullet() {
        let input = "- [ ] A checkbox item with enough text to need wrapping.";
        let expected = "- [ ] A checkbox item with enough text to
  need wrapping.
";
        assert_eq!(format_org(input, &wrap_lists_options(42)), expected);
    }

    #[test]
    fn test_wrap_lists_separate_paragraph_in_item() {
        let input = "- Item text.

  A second paragraph in the item that is long enough to wrap.";
        let options = FormatOptions {
            wrap_paragraphs: true,
            ..wrap_lists_options(40)
        };
        let expected = "- Item text.

  A second paragraph in the item that is
  long enough to wrap.
";
        assert_eq!(format_org(input, &options), expected);
    }

    #[test]
    fn test_wrap_lists_with_left_indent() {
        let input = "* Heading
- Parent item with enough text to need wrapping.
  continued
  - Child item with enough text to need wrapping.
Following paragraph text.";
        let options = FormatOptions {
            left_indent: true,
            wrap_paragraphs: true,
            ..wrap_lists_options(44)
        };
        let expected = "* Heading

\t- Parent item with enough text to
\t  need wrapping. continued
\t  - Child item with enough text to
\t    need wrapping.
\tFollowing paragraph text.
";
        let formatted = format_org(input, &options);
        assert_eq!(formatted, expected);
        assert_eq!(format_org(&formatted, &options), formatted);
    }

    #[test]
    fn test_wrap_paragraphs_dedent_starts_new_paragraph() {
        let input = "* H
  Indented paragraph text that wraps.
Dedented text is a separate paragraph.";
        let options = FormatOptions {
            left_indent: true,
            wrap_paragraphs: true,
            fill_column: 40,
            ..FormatOptions::default()
        };
        let expected = "* H

\t  Indented paragraph text that
\t  wraps.
\tDedented text is a separate
\tparagraph.
";
        let formatted = format_org(input, &options);
        assert_eq!(formatted, expected);
        assert_eq!(format_org(&formatted, &options), formatted);
    }

    fn left_indent_options() -> FormatOptions {
        FormatOptions {
            left_indent: true,
            ..FormatOptions::default()
        }
    }

    #[test]
    fn test_left_indent_content() {
        let input = "* Heading
:PROPERTIES:
:ID: abc
:END:
Some text.
- item
  - nested
    continuation
| a | b |
#+CAPTION: cap
#+begin_src sh
echo hi
#+end_src";
        let expected = "* Heading
:PROPERTIES:
:ID: abc
:END:

\tSome text.
\t- item
\t  - nested
\t    continuation
\t| a | b |
\t#+CAPTION: cap
\t#+BEGIN_SRC sh
echo hi
\t#+END_SRC
";
        assert_eq!(format_org(input, &left_indent_options()), expected);
    }

    #[test]
    fn test_left_indent_replaces_base_indent() {
        let input = "* Heading
  Some text.
  - item
    - nested
** Sub
\tAlready indented.
\t- item
\t  - nested";
        let expected = "* Heading

\tSome text.
\t- item
\t  - nested

** Sub

\tAlready indented.
\t- item
\t  - nested
";
        assert_eq!(format_org(input, &left_indent_options()), expected);
    }

    #[test]
    fn test_left_indent_preamble_keywords() {
        let input = "#+TITLE: Title
#+STARTUP: overview

Intro text.
* Heading
#+CAPTION: cap";
        let expected = "#+TITLE: Title
#+STARTUP: overview

\tIntro text.

* Heading

\t#+CAPTION: cap
";
        assert_eq!(format_org(input, &left_indent_options()), expected);
    }

    #[test]
    fn test_left_indent_wrapping() {
        let input = "* Heading
This paragraph is long enough that it must wrap at the fill column.
- A list item that is also long enough to wrap at the fill column.";
        let options = FormatOptions {
            left_indent: true,
            wrap_paragraphs: true,
            wrap_lists: true,
            fill_column: 40,
            ..FormatOptions::default()
        };
        let expected = "* Heading

\tThis paragraph is long enough
\tthat it must wrap at the fill
\tcolumn.
\t- A list item that is also long
\t  enough to wrap at the fill
\t  column.
";
        let formatted = format_org(input, &options);
        assert_eq!(formatted, expected);
        assert_eq!(format_org(&formatted, &options), formatted);
    }

    #[test]
    fn test_left_indent_idempotent() {
        let input = "#+TITLE: T
Intro.
* H
  text
  - a
    - b
#+begin_src sh :results output
  echo hi
#+end_src
#+begin_example
  raw
#+end_example
** H2
:PROPERTIES:
:X: y
:END:
| t |";
        let options = left_indent_options();
        let once = format_org(input, &options);
        assert_eq!(format_org(&once, &options), once);
    }
}
