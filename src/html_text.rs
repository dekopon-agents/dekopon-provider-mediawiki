use ego_tree::NodeRef;
use scraper::{Html, Node};

const MAX_DOM_NODES: usize = 50_000;
const MAX_DOM_DEPTH: usize = 256;
const MAX_FORMULA_CHARACTERS: usize = 256;
const MAX_FORMULA_BYTES: usize = 512;

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum HtmlTextError {
    TooComplex,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct PlainText {
    pub(crate) text: String,
    /// True when a formula was shortened or could not be represented.
    pub(crate) truncated: bool,
}

/// Parses an HTML fragment without recursion and never dereferences an embedded resource.
pub(crate) fn to_plain_text(fragment: &str) -> Result<PlainText, HtmlTextError> {
    let document = Html::parse_fragment(fragment);
    if document.tree.nodes().len() > MAX_DOM_NODES {
        return Err(HtmlTextError::TooComplex);
    }
    validate_dom_depth(document.tree.root())?;

    let mut output = TextBuilder::default();
    let mut stack = vec![Frame::Enter {
        node: document.tree.root(),
        depth: 0,
    }];
    while let Some(frame) = stack.pop() {
        match frame {
            Frame::ExitNewline => output.newline(),
            Frame::Enter { node, depth } => {
                if depth > MAX_DOM_DEPTH {
                    return Err(HtmlTextError::TooComplex);
                }
                let mut trailing_newline = false;
                match node.value() {
                    Node::Text(text) => output.push_text(text),
                    Node::Element(element) => {
                        if is_formula_container(element) {
                            let formula = formula_from_subtree(node, depth)?;
                            output.push_formula(formula);
                            continue;
                        }
                        if is_math_fallback_image(element) {
                            output.push_formula(formula_from_alt(element.attr("alt")));
                            continue;
                        }
                        if should_skip(element) {
                            continue;
                        }
                        match element.name() {
                            "br" | "hr" => output.newline(),
                            "li" => {
                                output.newline();
                                output.push_literal("- ");
                                trailing_newline = true;
                            }
                            "tr" => {
                                output.newline();
                                trailing_newline = true;
                            }
                            "td" | "th" => output.cell_boundary(),
                            name if is_block(name) => {
                                output.newline();
                                trailing_newline = true;
                            }
                            _ => {}
                        }
                    }
                    Node::Document
                    | Node::Fragment
                    | Node::Doctype(_)
                    | Node::Comment(_)
                    | Node::ProcessingInstruction(_) => {}
                }

                if trailing_newline {
                    stack.push(Frame::ExitNewline);
                }
                for child in node.children().rev() {
                    stack.push(Frame::Enter {
                        node: child,
                        depth: depth + 1,
                    });
                }
            }
        }
    }
    let truncated = output.truncated;
    Ok(PlainText {
        text: output.finish(),
        truncated,
    })
}

fn validate_dom_depth(root: NodeRef<'_, Node>) -> Result<(), HtmlTextError> {
    let mut stack = vec![(root, 0)];
    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_DOM_DEPTH {
            return Err(HtmlTextError::TooComplex);
        }
        for child in node.children().rev() {
            stack.push((child, depth + 1));
        }
    }
    Ok(())
}

/// Removes the repeated selected-section heading while preserving every nested heading.
pub(crate) fn remove_repeated_heading(text: String, heading: &str) -> String {
    let mut lines = text.lines();
    if lines
        .next()
        .is_some_and(|line| line.trim() == heading.trim())
    {
        lines.collect::<Vec<_>>().join("\n").trim().to_owned()
    } else {
        text
    }
}

#[derive(Clone, Copy)]
enum Frame<'a> {
    Enter {
        node: NodeRef<'a, Node>,
        depth: usize,
    },
    ExitNewline,
}

#[derive(Debug)]
struct Formula {
    text: Option<String>,
    truncated: bool,
}

fn formula_from_subtree(
    root: NodeRef<'_, Node>,
    root_depth: usize,
) -> Result<Formula, HtmlTextError> {
    let mut fallback = None;
    let mut stack = root
        .children()
        .rev()
        .map(|node| (node, root_depth + 1))
        .collect::<Vec<_>>();
    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_DOM_DEPTH {
            return Err(HtmlTextError::TooComplex);
        }
        if let Node::Element(element) = node.value() {
            if element.name() == "annotation"
                && element
                    .attr("encoding")
                    .is_some_and(|encoding| encoding.eq_ignore_ascii_case("application/x-tex"))
            {
                return descendant_formula_text(node, depth);
            }
            if fallback.is_none() && is_math_fallback_image(element) {
                fallback = Some(formula_from_alt(element.attr("alt")));
            }
        }
        for child in node.children().rev() {
            stack.push((child, depth + 1));
        }
    }
    Ok(fallback.unwrap_or(Formula {
        text: None,
        truncated: true,
    }))
}

fn descendant_formula_text(
    root: NodeRef<'_, Node>,
    root_depth: usize,
) -> Result<Formula, HtmlTextError> {
    let mut builder = TextBuilder::default();
    let mut stack = root
        .children()
        .rev()
        .map(|node| (node, root_depth + 1))
        .collect::<Vec<_>>();
    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_DOM_DEPTH {
            return Err(HtmlTextError::TooComplex);
        }
        if let Node::Text(text) = node.value() {
            builder.push_text(text);
        }
        for child in node.children().rev() {
            stack.push((child, depth + 1));
        }
    }
    Ok(bounded_formula(Some(builder.finish())))
}

fn formula_from_alt(alt: Option<&str>) -> Formula {
    bounded_formula(alt.map(str::to_owned))
}

fn bounded_formula(value: Option<String>) -> Formula {
    let Some(value) = value else {
        return Formula {
            text: None,
            truncated: true,
        };
    };
    let value = compact_text(&value);
    if value.is_empty() {
        return Formula {
            text: None,
            truncated: true,
        };
    }
    let (text, truncated) = truncate_text(&value, MAX_FORMULA_CHARACTERS, MAX_FORMULA_BYTES);
    Formula {
        text: Some(text),
        truncated,
    }
}

fn compact_text(value: &str) -> String {
    let mut builder = TextBuilder::default();
    builder.push_text(value);
    builder.finish()
}

fn truncate_text(value: &str, max_characters: usize, max_bytes: usize) -> (String, bool) {
    let mut end = 0;
    for (count, (index, character)) in value.char_indices().enumerate() {
        if count == max_characters || index + character.len_utf8() > max_bytes {
            break;
        }
        end = index + character.len_utf8();
    }
    (value[..end].to_owned(), end < value.len())
}

fn is_formula_container(element: &scraper::node::Element) -> bool {
    element.name() == "math" || element.classes().any(|class| class == "mwe-math-element")
}

fn is_math_fallback_image(element: &scraper::node::Element) -> bool {
    element.name() == "img"
        && element
            .classes()
            .any(|class| class.starts_with("mwe-math-fallback-image"))
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "dd"
            | "details"
            | "div"
            | "dl"
            | "dt"
            | "figcaption"
            | "figure"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "main"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "summary"
            | "table"
            | "tbody"
            | "tfoot"
            | "thead"
            | "ul"
    )
}

fn should_skip(element: &scraper::node::Element) -> bool {
    if matches!(
        element.name(),
        "audio"
            | "button"
            | "canvas"
            | "embed"
            | "form"
            | "iframe"
            | "img"
            | "input"
            | "link"
            | "math"
            | "meta"
            | "noscript"
            | "object"
            | "option"
            | "picture"
            | "script"
            | "select"
            | "source"
            | "style"
            | "svg"
            | "template"
            | "textarea"
            | "video"
    ) {
        return true;
    }
    if element
        .attr("role")
        .is_some_and(|role| role.eq_ignore_ascii_case("navigation"))
        || element
            .attr("typeof")
            .is_some_and(|kind| kind.contains("mw:Extension/ref"))
    {
        return true;
    }
    element.classes().any(|class| {
        let class = class.to_ascii_lowercase();
        class.contains("navbox")
            || matches!(
                class.as_str(),
                "authority-control"
                    | "metadata"
                    | "mw-editsection"
                    | "mw-empty-elt"
                    | "mw-jump-link"
                    | "mw-references-wrap"
                    | "noprint"
                    | "reference"
                    | "references"
                    | "reflist"
                    | "sistersitebox"
            )
    })
}

#[derive(Default)]
struct TextBuilder {
    text: String,
    pending_space: bool,
    truncated: bool,
}

impl TextBuilder {
    fn push_text(&mut self, value: &str) {
        for character in value.chars() {
            if character.is_whitespace() || character.is_control() {
                self.pending_space = !self.text.is_empty() && !self.text.ends_with('\n');
                continue;
            }
            if self.pending_space
                && !self.text.ends_with([' ', '\n'])
                && !matches!(
                    character,
                    '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}'
                )
            {
                self.text.push(' ');
            }
            self.pending_space = false;
            self.text.push(character);
        }
    }

    fn push_formula(&mut self, formula: Formula) {
        match formula.text {
            Some(text) => {
                self.push_literal("[formula: ");
                self.push_text(&text);
                if formula.truncated {
                    self.push_literal("…");
                }
                self.push_literal("]");
            }
            None => self.push_literal("[formula omitted]"),
        }
        self.truncated |= formula.truncated;
    }

    fn push_literal(&mut self, value: &str) {
        if self.pending_space && !self.text.ends_with([' ', '\n']) {
            self.text.push(' ');
        }
        self.pending_space = false;
        self.text.push_str(value);
    }

    fn newline(&mut self) {
        self.pending_space = false;
        while self.text.ends_with(' ') {
            self.text.pop();
        }
        if !self.text.is_empty() && !self.text.ends_with('\n') {
            self.text.push('\n');
        }
    }

    fn cell_boundary(&mut self) {
        self.pending_space = false;
        if !self.text.is_empty() && !self.text.ends_with(['\n', ' ']) {
            self.text.push_str(" | ");
        }
    }

    fn finish(mut self) -> String {
        while self.text.ends_with(char::is_whitespace) {
            self.text.pop();
        }
        self.text.trim().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{HtmlTextError, remove_repeated_heading, to_plain_text};

    fn text(html: &str) -> String {
        to_plain_text(html).expect("HTML is bounded").text
    }

    #[test]
    fn decodes_entities_and_search_markup() {
        assert_eq!(
            text("Ada&nbsp;<span class=\"searchmatch\">Lovelace</span> &amp; Byron"),
            "Ada Lovelace & Byron"
        );
    }

    #[test]
    fn hostile_markup_is_dom_parsed_and_resources_are_never_exposed() {
        let text = text(include_str!("../tests/fixtures/adversarial.html"));
        for absent in [
            "fetch",
            "evil.example",
            "secret alt",
            "[1]",
            "edit",
            "Navigation leak",
        ] {
            assert!(!text.contains(absent), "unexpected {absent:?} in {text:?}");
        }
        for present in [
            "Life & work",
            "Ada wrote <notes> and linked words.",
            "- First",
            "- Second",
            "Year | Work",
            "1843 | Notes",
            "Malformed bold",
            "still useful",
        ] {
            assert!(text.contains(present), "missing {present:?} in {text:?}");
        }
    }

    #[test]
    fn formulas_use_bounded_tex_or_fallback_alt_without_fetching() {
        let tex = to_plain_text(
            r#"E = <span class="mwe-math-element"><math><semantics><annotation encoding="application/x-tex">E = mc^2</annotation></semantics></math><img class="mwe-math-fallback-image-inline" src="https://evil.example/equation.svg" alt="duplicate"></span>"#,
        )
        .expect("formula is bounded");
        assert_eq!(tex.text, "E = [formula: E = mc^2]");
        assert!(!tex.truncated);
        assert!(!tex.text.contains("evil.example"));

        let fallback = to_plain_text(
            r#"<span class="mwe-math-element"><img class="mwe-math-fallback-image-inline" src="never-fetched" alt="x^2 + y^2" /></span>"#,
        )
        .expect("fallback is bounded");
        assert_eq!(fallback.text, "[formula: x^2 + y^2]");
        assert!(!fallback.truncated);

        let omitted = to_plain_text("value <math><mrow></mrow></math>")
            .expect("empty formula is represented");
        assert_eq!(omitted.text, "value [formula omitted]");
        assert!(omitted.truncated);
    }

    #[test]
    fn excessive_dom_depth_is_a_structured_error_not_recursion() {
        let fragment = format!("{}text{}", "<div>".repeat(300), "</div>".repeat(300));
        assert_eq!(to_plain_text(&fragment), Err(HtmlTextError::TooComplex));
    }

    #[test]
    fn removes_only_the_repeated_top_heading() {
        assert_eq!(
            remove_repeated_heading("Biography\nChildhood\nText".to_owned(), "Biography"),
            "Childhood\nText"
        );
        assert_eq!(
            remove_repeated_heading("Text\nBiography".to_owned(), "Biography"),
            "Text\nBiography"
        );
    }
}
