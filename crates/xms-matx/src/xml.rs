//! A small XML reader, enough for MaterialX documents: elements, attributes
//! and comments, the five named entities and numeric character references.
//! Text between elements is not kept (MaterialX puts nothing there), nor are
//! processing instructions, CDATA or a DOCTYPE, which are skipped.

/// An element: its name, its attributes in document order, its children.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
}

impl Element {
    /// The value of an attribute, if it has it.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// Its children with a given element name.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
}

/// What went wrong reading a document, and where (a byte offset).
#[derive(Clone, Debug, PartialEq)]
pub struct XmlError {
    pub at: usize,
    pub what: String,
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "XML error at byte {}: {}", self.at, self.what)
    }
}

impl std::error::Error for XmlError {}

/// The root element of a document.
pub fn parse(text: &str) -> Result<Element, XmlError> {
    let mut r = Reader { s: text.as_bytes(), text, i: 0 };
    r.misc()?;
    if r.i >= r.s.len() {
        return Err(r.err("no root element"));
    }
    let root = r.element()?;
    r.misc()?;
    if r.i < r.s.len() {
        return Err(r.err("content after the root element"));
    }
    Ok(root)
}

struct Reader<'a> {
    s: &'a [u8],
    text: &'a str,
    i: usize,
}

impl<'a> Reader<'a> {
    fn err(&self, what: &str) -> XmlError {
        XmlError { at: self.i, what: what.to_string() }
    }

    fn starts(&self, p: &str) -> bool {
        self.s[self.i..].starts_with(p.as_bytes())
    }

    fn skip_ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    /// Skip past `end`, which must come.
    fn skip_past(&mut self, end: &str) -> Result<(), XmlError> {
        match self.text[self.i..].find(end) {
            Some(k) => {
                self.i += k + end.len();
                Ok(())
            }
            None => Err(self.err(&format!("missing {end}"))),
        }
    }

    /// Whitespace, comments, processing instructions and declarations.
    fn misc(&mut self) -> Result<(), XmlError> {
        loop {
            self.skip_ws();
            if self.starts("<!--") {
                self.skip_past("-->")?;
            } else if self.starts("<?") {
                self.skip_past("?>")?;
            } else if self.starts("<!") {
                self.skip_past(">")?;
            } else {
                return Ok(());
            }
        }
    }

    fn name(&mut self) -> Result<String, XmlError> {
        let start = self.i;
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if c.is_ascii_whitespace() || c == b'=' || c == b'>' || c == b'/' {
                break;
            }
            self.i += 1;
        }
        if self.i == start {
            return Err(self.err("expected a name"));
        }
        Ok(self.text[start..self.i].to_string())
    }

    fn element(&mut self) -> Result<Element, XmlError> {
        if !self.starts("<") {
            return Err(self.err("expected an element"));
        }
        self.i += 1;
        let mut el = Element { name: self.name()?, ..Default::default() };
        // Attributes, up to `>` or `/>`.
        loop {
            self.skip_ws();
            if self.starts("/>") {
                self.i += 2;
                return Ok(el);
            }
            if self.starts(">") {
                self.i += 1;
                break;
            }
            let key = self.name()?;
            self.skip_ws();
            if !self.starts("=") {
                return Err(self.err("expected = after an attribute name"));
            }
            self.i += 1;
            self.skip_ws();
            let quote = match self.s.get(self.i) {
                Some(b'"') => "\"",
                Some(b'\'') => "'",
                _ => return Err(self.err("expected a quoted attribute value")),
            };
            self.i += 1;
            let start = self.i;
            self.skip_past(quote)?;
            let value = unescape(&self.text[start..self.i - 1]);
            el.attrs.push((key, value));
        }
        // Content: children and comments; text is skipped.
        loop {
            if self.i >= self.s.len() {
                return Err(self.err(&format!("<{}> is not closed", el.name)));
            }
            if self.starts("</") {
                self.i += 2;
                let end = self.name()?;
                if end != el.name {
                    return Err(self.err(&format!("</{end}> closes <{}>", el.name)));
                }
                self.skip_ws();
                if !self.starts(">") {
                    return Err(self.err("expected >"));
                }
                self.i += 1;
                return Ok(el);
            }
            if self.starts("<![CDATA[") {
                self.skip_past("]]>")?;
            } else if self.starts("<!--") || self.starts("<?") || self.starts("<!") {
                self.misc()?;
            } else if self.starts("<") {
                el.children.push(self.element()?);
            } else {
                // Text: skipped.
                match self.text[self.i..].find('<') {
                    Some(k) => self.i += k,
                    None => self.i = self.s.len(),
                }
            }
        }
    }
}

/// Attribute text with its entities and character references replaced.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(k) = rest.find('&') {
        out.push_str(&rest[..k]);
        rest = &rest[k..];
        let Some(end) = rest.find(';') else { break };
        let ent = &rest[1..end];
        let ch = match ent {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Attribute text made safe to write between double quotes.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_elements_attributes_and_skips_the_rest() {
        let doc = r#"<?xml version="1.0"?>
<!-- a comment, with <tags> in it -->
<materialx version="1.39">
  <!-- another -->
  <nodedef name="ND_x" node='x'>
    <input name="a" type="float" value="1.0" />
    some text
    <output name="out" type="float"/>
  </nodedef>
  <typedef name="float" />
</materialx>
"#;
        let root = parse(doc).unwrap();
        assert_eq!(root.name, "materialx");
        assert_eq!(root.attr("version"), Some("1.39"));
        assert_eq!(root.children.len(), 2);
        let nd = &root.children[0];
        assert_eq!(nd.attr("node"), Some("x"));
        assert_eq!(nd.children_named("input").count(), 1);
        assert_eq!(nd.children[1].name, "output");
    }

    #[test]
    fn entities_round_trip() {
        assert_eq!(unescape("a &lt; b &amp;&amp; c &gt; d &quot;e&quot; &#65;&#x42;"), "a < b && c > d \"e\" AB");
        assert_eq!(unescape("&unknown; &"), "&unknown; &");
        let s = "x < \"y\" & z";
        assert_eq!(unescape(&escape(s)), s);
    }

    #[test]
    fn mismatched_or_unclosed_elements_are_errors() {
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a>").is_err());
        assert!(parse("").is_err());
        assert!(parse("<a/><b/>").is_err());
    }
}
