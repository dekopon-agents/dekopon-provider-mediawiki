use ego_tree::NodeRef;
use scraper::{Html, Node};

/// Parses an HTML fragment into bounded-call-site plaintext without dereferencing any resource.
pub(crate) fn to_plain_text(fragment: &str) -> String {
    let document = Html::parse_fragment(fragment);
    let mut output = TextBuilder::default();
    walk(document.tree.root(), &mut output);
    output.finish()
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

fn walk(node: NodeRef<'_, Node>, output: &mut TextBuilder) {
    let mut block = false;
    match node.value() {
        Node::Text(text) => output.push_text(text),
        Node::Element(element) => {
            if should_skip(element) {
                return;
            }
            let name = element.name();
            match name {
                "br" | "hr" => output.newline(),
                "li" => {
                    output.newline();
                    output.push_literal("- ");
                }
                "tr" => output.newline(),
                "td" | "th" => output.cell_boundary(),
                _ if is_block(name) => {
                    output.newline();
                    block = true;
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

    for child in node.children() {
        walk(child, output);
    }
    if block
        || matches!(node.value(), Node::Element(element) if matches!(element.name(), "li" | "tr"))
    {
        output.newline();
    }
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

    fn push_literal(&mut self, value: &str) {
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
    use super::{remove_repeated_heading, to_plain_text};

    #[test]
    fn decodes_entities_and_search_markup() {
        assert_eq!(
            to_plain_text("Ada&nbsp;<span class=\"searchmatch\">Lovelace</span> &amp; Byron"),
            "Ada Lovelace & Byron"
        );
    }

    #[test]
    fn hostile_markup_is_dom_parsed_and_resources_are_never_exposed() {
        let text = to_plain_text(include_str!("../tests/fixtures/adversarial.html"));
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
