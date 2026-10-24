//! The parts of expat 2.7.4, the XML parser `htmllex.c` hands HTML labels to, that labels
//! reach: the content tokenizer of `xmltok_impl.c` for UTF-8 and ISO-8859-1 (`contentTok`,
//! `scanLt`, `scanAtts`, `scanEndTag`, `scanRef`, `scanComment`, `scanPi`,
//! `cdataSectionTok`), the byte types and name tables of `xmltok.c` (`utf8tab.h`,
//! `latin1tab.h`, `asciitab.h`, `nametab.h`), and from `xmlparse.c` the buffer and reparse
//! deferral of `XML_Parse` (`XML_GetBuffer`, `callProcessor`), `doContent`,
//! `doCdataSection`, `epilogProcessor`, `storeAtts` with `appendAttributeValue`, and the
//! error position up to which `XML_GetCurrentLineNumber` counts lines.
//!
//! `htmllex` calls `XML_Parse` once per `findNext` unit and never with `isFinal`, so a token
//! that a unit leaves unfinished (a comment closed by `>` without `--`, a character cut by
//! the end of a unit) waits in the buffer and is tokenized with the units after it. Several
//! units' callbacks can then come from one `XML_Parse` call.
//!
//! Expat is Copyright (c) 1998-2000 Thai Open Source Software Center Ltd and Clark Cooper,
//! and Copyright (c) 2001-2025 Expat maintainers, under the MIT licence
//! (`LICENSES/MIT-EXPAT.txt`).

/// The encoding `XML_ParserCreate` is given (`charsetToStr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Enc {
    Utf8,
    Latin1,
}

/// Byte types (`BT_*` in `xmltok_impl.h`). Without namespace processing `BT_COLON` is
/// `BT_NMSTRT`; `BT_NONASCII` only occurs in UTF-16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bt {
    Nonxml,
    Malform,
    Lt,
    Amp,
    Rsqb,
    Lead2,
    Lead3,
    Lead4,
    Trail,
    Cr,
    Lf,
    Gt,
    Quot,
    Apos,
    Equals,
    Quest,
    Excl,
    Sol,
    Semi,
    Num,
    Lsqb,
    S,
    Nmstrt,
    Hex,
    Digit,
    Name,
    Minus,
    Other,
    Percnt,
    Lpar,
    Rpar,
    Ast,
    Plus,
    Comma,
    Verbar,
}

/// `asciitab.h`.
fn ascii_type(b: u8) -> Bt {
    match b {
        b'\t' | b' ' => Bt::S,
        b'\n' => Bt::Lf,
        b'\r' => Bt::Cr,
        0..=0x1F => Bt::Nonxml,
        b'!' => Bt::Excl,
        b'"' => Bt::Quot,
        b'#' => Bt::Num,
        b'%' => Bt::Percnt,
        b'&' => Bt::Amp,
        b'\'' => Bt::Apos,
        b'(' => Bt::Lpar,
        b')' => Bt::Rpar,
        b'*' => Bt::Ast,
        b'+' => Bt::Plus,
        b',' => Bt::Comma,
        b'-' => Bt::Minus,
        b'.' => Bt::Name,
        b'/' => Bt::Sol,
        b'0'..=b'9' => Bt::Digit,
        b';' => Bt::Semi,
        b'<' => Bt::Lt,
        b'=' => Bt::Equals,
        b'>' => Bt::Gt,
        b'?' => Bt::Quest,
        b'A'..=b'F' | b'a'..=b'f' => Bt::Hex,
        b':' | b'G'..=b'Z' | b'g'..=b'z' | b'_' => Bt::Nmstrt,
        b'[' => Bt::Lsqb,
        b']' => Bt::Rsqb,
        b'|' => Bt::Verbar,
        _ => Bt::Other,
    }
}

fn byte_type(enc: Enc, b: u8) -> Bt {
    if b < 0x80 {
        return ascii_type(b);
    }
    match enc {
        // utf8tab.h
        Enc::Utf8 => match b {
            0x80..=0xBF => Bt::Trail,
            0xC0..=0xDF => Bt::Lead2,
            0xE0..=0xEF => Bt::Lead3,
            0xF0..=0xF4 => Bt::Lead4,
            0xF5..=0xFD => Bt::Nonxml,
            _ => Bt::Malform,
        },
        // latin1tab.h
        Enc::Latin1 => match b {
            0xAA | 0xB5 | 0xBA | 0xC0..=0xD6 | 0xD8..=0xF6 | 0xF8..=0xFF => Bt::Nmstrt,
            0xB7 => Bt::Name,
            _ => Bt::Other,
        },
    }
}

/// `UTF8_INVALID2..4` (`xmltok.c`): a lead byte's sequence that is not UTF-8, or is U+FFFE
/// or U+FFFF.
fn utf8_invalid(p: &[u8]) -> bool {
    let tail = |b: u8| b & 0xC0 != 0x80;
    match p.len() {
        2 => p[0] < 0xC2 || tail(p[1]),
        3 => {
            p[2] & 0x80 == 0
                || if p[0] == 0xEF && p[1] == 0xBF {
                    p[2] > 0xBD
                } else {
                    p[2] & 0xC0 == 0xC0
                }
                || if p[0] == 0xE0 {
                    p[1] < 0xA0 || p[1] & 0xC0 == 0xC0
                } else {
                    p[1] & 0x80 == 0
                        || if p[0] == 0xED {
                            p[1] > 0x9F
                        } else {
                            p[1] & 0xC0 == 0xC0
                        }
                }
        }
        _ => {
            tail(p[3])
                || tail(p[2])
                || if p[0] == 0xF0 {
                    p[1] < 0x90 || p[1] & 0xC0 == 0xC0
                } else {
                    p[1] & 0x80 == 0
                        || if p[0] == 0xF4 {
                            p[1] > 0x8F
                        } else {
                            p[1] & 0xC0 == 0xC0
                        }
                }
        }
    }
}

/// `UTF8_GET_NAMING2`/`UTF8_GET_NAMING3` (`xmltok.c`); four-byte characters are never name
/// characters (`isNever`).
fn utf8_naming(pages: &[u8; 256], p: &[u8]) -> bool {
    let (page, word, bit) = match p.len() {
        2 => (
            pages[usize::from((p[0] >> 2) & 7)],
            usize::from(p[0] & 3) << 1 | usize::from((p[1] >> 5) & 1),
            p[1] & 0x1F,
        ),
        3 => (
            pages[usize::from(p[0] & 0xF) << 4 | usize::from((p[1] >> 2) & 0xF)],
            usize::from(p[1] & 3) << 1 | usize::from((p[2] >> 5) & 1),
            p[2] & 0x1F,
        ),
        _ => return false,
    };
    NAMING_BITMAP[(usize::from(page) << 3) + word] & (1u32 << bit) != 0
}

// Tokens (`XML_TOK_*` in `xmltok.h`); the `usize` is `*nextTokPtr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum T {
    None,
    Partial,
    PartialChar,
    Invalid(usize),
    TrailingCr,
    TrailingRsqb,
    DataChars(usize),
    DataNewline(usize),
    StartTag(usize),
    EmptyElement(usize),
    EndTag(usize),
    CharRef(usize),
    EntityRef(usize),
    Comment(usize),
    Pi(usize),
    XmlDecl(usize),
    CdataOpen(usize),
    CdataClose(usize),
    /// The epilog's tokens other than space, comments and PIs, which are all junk.
    Junk,
}

/// A tokenizer over the bytes `d[..end]` expat has been given.
struct Tk<'a> {
    d: &'a [u8],
    enc: Enc,
}

impl Tk<'_> {
    fn end(&self) -> usize {
        self.d.len()
    }

    fn bt(&self, p: usize) -> Bt {
        byte_type(self.enc, self.d[p])
    }

    fn has(&self, p: usize, n: usize) -> bool {
        self.end().saturating_sub(p) >= n
    }

    fn is(&self, p: usize, c: u8) -> bool {
        self.d[p] == c
    }

    fn lead_len(bt: Bt) -> Option<usize> {
        match bt {
            Bt::Lead2 => Some(2),
            Bt::Lead3 => Some(3),
            Bt::Lead4 => Some(4),
            _ => None,
        }
    }

    /// `INVALID_CASES`: `Some(Ok(next))` past a valid multi-byte character, `Some(Err(tok))`
    /// to return, `None` if the byte type is not one of them.
    fn invalid_case(&self, p: usize) -> Option<Result<usize, T>> {
        let bt = self.bt(p);
        if let Some(n) = Self::lead_len(bt) {
            if !self.has(p, n) {
                return Some(Err(T::PartialChar));
            }
            if utf8_invalid(&self.d[p..p + n]) {
                return Some(Err(T::Invalid(p)));
            }
            return Some(Ok(p + n));
        }
        match bt {
            Bt::Nonxml | Bt::Malform | Bt::Trail => Some(Err(T::Invalid(p))),
            _ => None,
        }
    }

    /// `CHECK_NAME_CASES` (`start == false`) and `CHECK_NMSTRT_CASES`.
    fn name_case(&self, p: usize, start: bool) -> Option<Result<usize, T>> {
        let bt = self.bt(p);
        if let Some(n) = Self::lead_len(bt) {
            if !self.has(p, n) {
                return Some(Err(T::PartialChar));
            }
            let c = &self.d[p..p + n];
            let pages = if start { &NMSTRT_PAGES } else { &NAME_PAGES };
            if utf8_invalid(c) || !utf8_naming(pages, c) {
                return Some(Err(T::Invalid(p)));
            }
            return Some(Ok(p + n));
        }
        match bt {
            Bt::Nmstrt | Bt::Hex => Some(Ok(p + 1)),
            Bt::Digit | Bt::Name | Bt::Minus if !start => Some(Ok(p + 1)),
            _ => None,
        }
    }

    /// `scanComment`: `p` follows `<!-`.
    fn scan_comment(&self, mut p: usize) -> T {
        if self.has(p, 1) {
            if !self.is(p, b'-') {
                return T::Invalid(p);
            }
            p += 1;
            while self.has(p, 1) {
                if let Some(r) = self.invalid_case(p) {
                    match r {
                        Ok(q) => p = q,
                        Err(t) => return t,
                    }
                    continue;
                }
                if self.bt(p) == Bt::Minus {
                    p += 1;
                    if !self.has(p, 1) {
                        return T::Partial;
                    }
                    if self.is(p, b'-') {
                        p += 1;
                        if !self.has(p, 1) {
                            return T::Partial;
                        }
                        if !self.is(p, b'>') {
                            return T::Invalid(p);
                        }
                        return T::Comment(p + 1);
                    }
                } else {
                    p += 1;
                }
            }
        }
        T::Partial
    }

    /// `checkPiTarget`: `None` for a target `xml` with an upper-case letter (rejected),
    /// otherwise whether the target is `xml`.
    fn pi_target_is_xml(&self, target: &[u8]) -> Option<bool> {
        if target.len() != 3 || !target.eq_ignore_ascii_case(b"xml") {
            return Some(false);
        }
        if target != b"xml" {
            return None;
        }
        Some(true)
    }

    /// `scanPi`: `p` follows `<?`.
    fn scan_pi(&self, mut p: usize) -> T {
        let target = p;
        if !self.has(p, 1) {
            return T::Partial;
        }
        match self.name_case(p, true) {
            Some(Ok(q)) => p = q,
            Some(Err(t)) => return t,
            None => return T::Invalid(p),
        }
        while self.has(p, 1) {
            match self.name_case(p, false) {
                Some(Ok(q)) => {
                    p = q;
                    continue;
                }
                Some(Err(t)) => return t,
                None => {}
            }
            let tok = |xml: bool, n: usize| if xml { T::XmlDecl(n) } else { T::Pi(n) };
            match self.bt(p) {
                Bt::S | Bt::Cr | Bt::Lf => {
                    let Some(xml) = self.pi_target_is_xml(&self.d[target..p]) else {
                        return T::Invalid(p);
                    };
                    p += 1;
                    while self.has(p, 1) {
                        if let Some(r) = self.invalid_case(p) {
                            match r {
                                Ok(q) => p = q,
                                Err(t) => return t,
                            }
                            continue;
                        }
                        if self.bt(p) == Bt::Quest {
                            p += 1;
                            if !self.has(p, 1) {
                                return T::Partial;
                            }
                            if self.is(p, b'>') {
                                return tok(xml, p + 1);
                            }
                        } else {
                            p += 1;
                        }
                    }
                    return T::Partial;
                }
                Bt::Quest => {
                    let Some(xml) = self.pi_target_is_xml(&self.d[target..p]) else {
                        return T::Invalid(p);
                    };
                    p += 1;
                    if !self.has(p, 1) {
                        return T::Partial;
                    }
                    if self.is(p, b'>') {
                        return tok(xml, p + 1);
                    }
                    return T::Invalid(p);
                }
                _ => return T::Invalid(p),
            }
        }
        T::Partial
    }

    /// `scanCdataSection`: `p` follows `<![`.
    fn scan_cdata_section(&self, p: usize) -> T {
        if !self.has(p, 6) {
            return T::Partial;
        }
        for (i, &c) in b"CDATA[".iter().enumerate() {
            if !self.is(p + i, c) {
                return T::Invalid(p + i);
            }
        }
        T::CdataOpen(p + 6)
    }

    /// `cdataSectionTok`.
    fn cdata_section_tok(&self, mut p: usize) -> T {
        if p >= self.end() {
            return T::None;
        }
        match self.bt(p) {
            Bt::Rsqb => {
                p += 1;
                if !self.has(p, 1) {
                    return T::Partial;
                }
                if self.is(p, b']') {
                    p += 1;
                    if !self.has(p, 1) {
                        return T::Partial;
                    }
                    if self.is(p, b'>') {
                        return T::CdataClose(p + 1);
                    }
                    p -= 1;
                }
            }
            Bt::Cr => {
                p += 1;
                if !self.has(p, 1) {
                    return T::Partial;
                }
                if self.bt(p) == Bt::Lf {
                    p += 1;
                }
                return T::DataNewline(p);
            }
            Bt::Lf => return T::DataNewline(p + 1),
            _ => match self.invalid_case(p) {
                Some(Ok(q)) => p = q,
                Some(Err(t)) => return t,
                None => p += 1,
            },
        }
        self.data_chars(p, false)
    }

    /// The data-character loops of `contentTok` (`content`) and `cdataSectionTok`.
    fn data_chars(&self, mut p: usize, content: bool) -> T {
        while self.has(p, 1) {
            let bt = self.bt(p);
            if let Some(n) = Self::lead_len(bt) {
                if !self.has(p, n) || utf8_invalid(&self.d[p..p + n]) {
                    return T::DataChars(p);
                }
                p += n;
                continue;
            }
            match bt {
                Bt::Rsqb if content => {
                    if self.has(p, 2) {
                        if !self.is(p + 1, b']') {
                            p += 1;
                            continue;
                        }
                        if self.has(p, 3) {
                            if !self.is(p + 2, b'>') {
                                p += 1;
                                continue;
                            }
                            return T::Invalid(p + 2);
                        }
                    }
                    return T::DataChars(p);
                }
                Bt::Amp | Bt::Lt if content => return T::DataChars(p),
                Bt::Rsqb | Bt::Nonxml | Bt::Malform | Bt::Trail | Bt::Cr | Bt::Lf => {
                    return T::DataChars(p);
                }
                _ => p += 1,
            }
        }
        T::DataChars(p)
    }

    /// `scanEndTag`: `p` follows `</`.
    fn scan_end_tag(&self, mut p: usize) -> T {
        if !self.has(p, 1) {
            return T::Partial;
        }
        match self.name_case(p, true) {
            Some(Ok(q)) => p = q,
            Some(Err(t)) => return t,
            None => return T::Invalid(p),
        }
        while self.has(p, 1) {
            match self.name_case(p, false) {
                Some(Ok(q)) => {
                    p = q;
                    continue;
                }
                Some(Err(t)) => return t,
                None => {}
            }
            match self.bt(p) {
                Bt::S | Bt::Cr | Bt::Lf => {
                    p += 1;
                    while self.has(p, 1) {
                        match self.bt(p) {
                            Bt::S | Bt::Cr | Bt::Lf => p += 1,
                            Bt::Gt => return T::EndTag(p + 1),
                            _ => return T::Invalid(p),
                        }
                    }
                    return T::Partial;
                }
                Bt::Gt => return T::EndTag(p + 1),
                _ => return T::Invalid(p),
            }
        }
        T::Partial
    }

    /// `scanCharRef` and `scanHexCharRef`: `p` follows `&#`.
    fn scan_char_ref(&self, mut p: usize) -> T {
        if !self.has(p, 1) {
            return T::Partial;
        }
        let hex = self.is(p, b'x');
        if hex {
            p += 1;
            if !self.has(p, 1) {
                return T::Partial;
            }
        }
        let digit = |bt: Bt| bt == Bt::Digit || (hex && bt == Bt::Hex);
        if !digit(self.bt(p)) {
            return T::Invalid(p);
        }
        p += 1;
        while self.has(p, 1) {
            match self.bt(p) {
                bt if digit(bt) => p += 1,
                Bt::Semi => return T::CharRef(p + 1),
                _ => return T::Invalid(p),
            }
        }
        T::Partial
    }

    /// `scanRef`: `p` follows `&`.
    fn scan_ref(&self, mut p: usize) -> T {
        if !self.has(p, 1) {
            return T::Partial;
        }
        match self.name_case(p, true) {
            Some(Ok(q)) => p = q,
            Some(Err(t)) => return t,
            None if self.bt(p) == Bt::Num => return self.scan_char_ref(p + 1),
            None => return T::Invalid(p),
        }
        while self.has(p, 1) {
            match self.name_case(p, false) {
                Some(Ok(q)) => p = q,
                Some(Err(t)) => return t,
                None if self.bt(p) == Bt::Semi => return T::EntityRef(p + 1),
                None => return T::Invalid(p),
            }
        }
        T::Partial
    }

    /// `scanAtts`: `p` follows the first character of an attribute name.
    fn scan_atts(&self, mut p: usize) -> T {
        while self.has(p, 1) {
            match self.name_case(p, false) {
                Some(Ok(q)) => {
                    p = q;
                    continue;
                }
                Some(Err(t)) => return t,
                None => {}
            }
            match self.bt(p) {
                Bt::S | Bt::Cr | Bt::Lf | Bt::Equals => {
                    if self.bt(p) != Bt::Equals {
                        loop {
                            p += 1;
                            if !self.has(p, 1) {
                                return T::Partial;
                            }
                            match self.bt(p) {
                                Bt::Equals => break,
                                Bt::S | Bt::Cr | Bt::Lf => {}
                                _ => return T::Invalid(p),
                            }
                        }
                    }
                    let open = loop {
                        p += 1;
                        if !self.has(p, 1) {
                            return T::Partial;
                        }
                        match self.bt(p) {
                            o @ (Bt::Quot | Bt::Apos) => break o,
                            Bt::S | Bt::Cr | Bt::Lf => {}
                            _ => return T::Invalid(p),
                        }
                    };
                    p += 1;
                    // in the attribute value
                    loop {
                        if !self.has(p, 1) {
                            return T::Partial;
                        }
                        let t = self.bt(p);
                        if t == open {
                            break;
                        }
                        if let Some(r) = self.invalid_case(p) {
                            match r {
                                Ok(q) => p = q,
                                Err(t) => return t,
                            }
                            continue;
                        }
                        match t {
                            Bt::Amp => match self.scan_ref(p + 1) {
                                T::EntityRef(q) | T::CharRef(q) => p = q,
                                t => return t,
                            },
                            Bt::Lt => return T::Invalid(p),
                            _ => p += 1,
                        }
                    }
                    p += 1;
                    if !self.has(p, 1) {
                        return T::Partial;
                    }
                    match self.bt(p) {
                        Bt::S | Bt::Cr | Bt::Lf => {}
                        Bt::Sol => return self.sol(p),
                        Bt::Gt => return T::StartTag(p + 1),
                        _ => return T::Invalid(p),
                    }
                    // after the closing quote and a space
                    loop {
                        p += 1;
                        if !self.has(p, 1) {
                            return T::Partial;
                        }
                        match self.name_case(p, true) {
                            Some(Ok(q)) => {
                                p = q;
                                break;
                            }
                            Some(Err(t)) => return t,
                            None => {}
                        }
                        match self.bt(p) {
                            Bt::S | Bt::Cr | Bt::Lf => {}
                            Bt::Gt => return T::StartTag(p + 1),
                            Bt::Sol => return self.sol(p),
                            _ => return T::Invalid(p),
                        }
                    }
                }
                _ => return T::Invalid(p),
            }
        }
        T::Partial
    }

    /// The `sol:` label: `p` is at `/` in a start tag.
    fn sol(&self, mut p: usize) -> T {
        p += 1;
        if !self.has(p, 1) {
            return T::Partial;
        }
        if !self.is(p, b'>') {
            return T::Invalid(p);
        }
        T::EmptyElement(p + 1)
    }

    /// `scanLt`: `p` follows `<`.
    fn scan_lt(&self, mut p: usize) -> T {
        if !self.has(p, 1) {
            return T::Partial;
        }
        match self.name_case(p, true) {
            Some(Ok(q)) => p = q,
            Some(Err(t)) => return t,
            None => match self.bt(p) {
                Bt::Excl => {
                    p += 1;
                    if !self.has(p, 1) {
                        return T::Partial;
                    }
                    return match self.bt(p) {
                        Bt::Minus => self.scan_comment(p + 1),
                        Bt::Lsqb => self.scan_cdata_section(p + 1),
                        _ => T::Invalid(p),
                    };
                }
                Bt::Quest => return self.scan_pi(p + 1),
                Bt::Sol => return self.scan_end_tag(p + 1),
                _ => return T::Invalid(p),
            },
        }
        // a start tag
        while self.has(p, 1) {
            match self.name_case(p, false) {
                Some(Ok(q)) => {
                    p = q;
                    continue;
                }
                Some(Err(t)) => return t,
                None => {}
            }
            match self.bt(p) {
                Bt::S | Bt::Cr | Bt::Lf => {
                    p += 1;
                    while self.has(p, 1) {
                        match self.name_case(p, true) {
                            Some(Ok(q)) => return self.scan_atts(q),
                            Some(Err(t)) => return t,
                            None => {}
                        }
                        match self.bt(p) {
                            Bt::Gt => return T::StartTag(p + 1),
                            Bt::Sol => return self.sol(p),
                            Bt::S | Bt::Cr | Bt::Lf => p += 1,
                            _ => return T::Invalid(p),
                        }
                    }
                    return T::Partial;
                }
                Bt::Gt => return T::StartTag(p + 1),
                Bt::Sol => return self.sol(p),
                _ => return T::Invalid(p),
            }
        }
        T::Partial
    }

    /// `contentTok`.
    fn content_tok(&self, mut p: usize) -> T {
        if p >= self.end() {
            return T::None;
        }
        match self.bt(p) {
            Bt::Lt => return self.scan_lt(p + 1),
            Bt::Amp => return self.scan_ref(p + 1),
            Bt::Cr => {
                p += 1;
                if !self.has(p, 1) {
                    return T::TrailingCr;
                }
                if self.bt(p) == Bt::Lf {
                    p += 1;
                }
                return T::DataNewline(p);
            }
            Bt::Lf => return T::DataNewline(p + 1),
            Bt::Rsqb => {
                p += 1;
                if !self.has(p, 1) {
                    return T::TrailingRsqb;
                }
                if self.is(p, b']') {
                    p += 1;
                    if !self.has(p, 1) {
                        return T::TrailingRsqb;
                    }
                    if self.is(p, b'>') {
                        return T::Invalid(p);
                    }
                    p -= 1;
                }
            }
            _ => match self.invalid_case(p) {
                Some(Ok(q)) => p = q,
                Some(Err(t)) => return t,
                None => p += 1,
            },
        }
        self.data_chars(p, true)
    }

    /// `prologTok`, as `epilogProcessor` sees it: space (`DataNewline`), comments and PIs,
    /// which it accepts; `Invalid`, `Partial` and `PartialChar`; and every other token,
    /// complete or cut off by the end of the buffer, as `Junk`.
    fn epilog_tok(&self, mut p: usize) -> T {
        if p >= self.end() {
            return T::None;
        }
        match self.bt(p) {
            Bt::Quot | Bt::Apos => self.scan_lit(p + 1, self.bt(p)),
            Bt::Lt => {
                p += 1;
                if !self.has(p, 1) {
                    return T::Partial;
                }
                match self.bt(p) {
                    Bt::Excl => self.scan_decl(p + 1),
                    Bt::Quest => match self.scan_pi(p + 1) {
                        T::XmlDecl(_) => T::Junk,
                        t => t,
                    },
                    // XML_TOK_INSTANCE_START
                    Bt::Nmstrt | Bt::Hex | Bt::Lead2 | Bt::Lead3 | Bt::Lead4 => T::Junk,
                    _ => T::Invalid(p),
                }
            }
            Bt::S | Bt::Lf | Bt::Cr => {
                while self.has(p, 1) && matches!(self.bt(p), Bt::S | Bt::Lf | Bt::Cr) {
                    p += 1;
                }
                T::DataNewline(p)
            }
            Bt::Percnt => self.scan_percent(p + 1),
            Bt::Comma | Bt::Lsqb | Bt::Lpar | Bt::Verbar | Bt::Gt => T::Junk,
            Bt::Rsqb => {
                p += 1;
                if self.has(p, 1) && self.is(p, b']') && !self.has(p, 2) {
                    return T::Partial;
                }
                T::Junk
            }
            Bt::Rpar => {
                p += 1;
                if !self.has(p, 1) {
                    return T::Junk;
                }
                match self.bt(p) {
                    Bt::Ast | Bt::Quest | Bt::Plus => T::Junk,
                    Bt::Cr | Bt::Lf | Bt::S | Bt::Gt | Bt::Comma | Bt::Verbar | Bt::Rpar => T::Junk,
                    _ => T::Invalid(p),
                }
            }
            Bt::Num => {
                // scanPoundName
                p += 1;
                if !self.has(p, 1) {
                    return T::Partial;
                }
                match self.name_case(p, true) {
                    Some(Ok(q)) => p = q,
                    Some(Err(t)) => return t,
                    None => return T::Invalid(p),
                }
                while self.has(p, 1) {
                    match self.name_case(p, false) {
                        Some(Ok(q)) => p = q,
                        Some(Err(t)) => return t,
                        None => {
                            return match self.bt(p) {
                                Bt::Cr
                                | Bt::Lf
                                | Bt::S
                                | Bt::Rpar
                                | Bt::Gt
                                | Bt::Percnt
                                | Bt::Verbar => T::Junk,
                                _ => T::Invalid(p),
                            };
                        }
                    }
                }
                T::Junk
            }
            bt => {
                // a name (XML_TOK_NAME) or a name token (XML_TOK_NMTOKEN)
                let name = match Self::lead_len(bt) {
                    Some(n) => {
                        if !self.has(p, n) {
                            return T::PartialChar;
                        }
                        let c = &self.d[p..p + n];
                        if utf8_invalid(c) {
                            return T::Invalid(p);
                        }
                        let name = if utf8_naming(&NMSTRT_PAGES, c) {
                            true
                        } else if utf8_naming(&NAME_PAGES, c) {
                            false
                        } else {
                            return T::Invalid(p);
                        };
                        p += n;
                        name
                    }
                    None => match bt {
                        Bt::Nmstrt | Bt::Hex => {
                            p += 1;
                            true
                        }
                        Bt::Digit | Bt::Name | Bt::Minus => {
                            p += 1;
                            false
                        }
                        _ => return T::Invalid(p),
                    },
                };
                while self.has(p, 1) {
                    match self.name_case(p, false) {
                        Some(Ok(q)) => p = q,
                        Some(Err(t)) => return t,
                        None => {
                            return match self.bt(p) {
                                Bt::Gt
                                | Bt::Rpar
                                | Bt::Comma
                                | Bt::Verbar
                                | Bt::Lsqb
                                | Bt::Percnt
                                | Bt::S
                                | Bt::Cr
                                | Bt::Lf => T::Junk,
                                Bt::Plus | Bt::Ast | Bt::Quest if name => T::Junk,
                                _ => T::Invalid(p),
                            };
                        }
                    }
                }
                T::Junk
            }
        }
    }

    /// `scanLit`: `p` follows the opening quote `open`.
    fn scan_lit(&self, mut p: usize, open: Bt) -> T {
        while self.has(p, 1) {
            if let Some(r) = self.invalid_case(p) {
                match r {
                    Ok(q) => p = q,
                    Err(t) => return t,
                }
                continue;
            }
            let t = self.bt(p);
            p += 1;
            if t == open {
                if !self.has(p, 1) {
                    return T::Junk;
                }
                return match self.bt(p) {
                    Bt::S | Bt::Cr | Bt::Lf | Bt::Gt | Bt::Percnt | Bt::Lsqb => T::Junk,
                    _ => T::Invalid(p),
                };
            }
        }
        T::Partial
    }

    /// `scanDecl`: `p` follows `<!`.
    fn scan_decl(&self, mut p: usize) -> T {
        if !self.has(p, 1) {
            return T::Partial;
        }
        match self.bt(p) {
            Bt::Minus => return self.scan_comment(p + 1),
            Bt::Lsqb => return T::Junk,
            Bt::Nmstrt | Bt::Hex => p += 1,
            _ => return T::Invalid(p),
        }
        while self.has(p, 1) {
            match self.bt(p) {
                Bt::Percnt => {
                    if !self.has(p, 2) {
                        return T::Partial;
                    }
                    if matches!(self.bt(p + 1), Bt::S | Bt::Cr | Bt::Lf | Bt::Percnt) {
                        return T::Invalid(p);
                    }
                    return T::Junk;
                }
                Bt::S | Bt::Cr | Bt::Lf => return T::Junk,
                Bt::Nmstrt | Bt::Hex => p += 1,
                _ => return T::Invalid(p),
            }
        }
        T::Partial
    }

    /// `scanPercent`: `p` follows `%`.
    fn scan_percent(&self, mut p: usize) -> T {
        if !self.has(p, 1) {
            return T::Partial;
        }
        match self.name_case(p, true) {
            Some(Ok(q)) => p = q,
            Some(Err(t)) => return t,
            None => {
                return match self.bt(p) {
                    Bt::S | Bt::Lf | Bt::Cr | Bt::Percnt => T::Junk,
                    _ => T::Invalid(p),
                };
            }
        }
        while self.has(p, 1) {
            match self.name_case(p, false) {
                Some(Ok(q)) => p = q,
                Some(Err(t)) => return t,
                None if self.bt(p) == Bt::Semi => return T::Junk,
                None => return T::Invalid(p),
            }
        }
        T::Partial
    }

    /// `XmlNameLength`.
    fn name_length(&self, mut p: usize) -> usize {
        let start = p;
        while p < self.end() {
            match self.bt(p) {
                Bt::Nmstrt | Bt::Hex | Bt::Digit | Bt::Name | Bt::Minus => p += 1,
                bt => match Self::lead_len(bt) {
                    Some(n) => p += n,
                    None => break,
                },
            }
        }
        p - start
    }

    /// `XmlCharRefNumber` with `checkCharRefNumber`: `s` is at `&#`.
    fn char_ref_number(&self, s: usize, next: usize) -> Option<char> {
        let body = &self.d[s + 2..next - 1];
        let (digits, radix) = match body.strip_prefix(b"x") {
            Some(h) => (h, 16),
            None => (body, 10),
        };
        let mut n: u32 = 0;
        for &c in digits {
            n = n * radix + char::from(c).to_digit(radix)?;
            if n >= 0x11_0000 {
                return None;
            }
        }
        match n >> 8 {
            0xD8..=0xDF => return None,
            0 if ascii_type(n as u8) == Bt::Nonxml => return None,
            0xFF if n >= 0xFFFE => return None,
            _ => {}
        }
        char::from_u32(n)
    }

    /// `XmlPredefinedEntityName`: `s` is at `&`, `next` after `;`.
    fn predefined_entity(&self, s: usize, next: usize) -> Option<u8> {
        Some(match &self.d[s + 1..next - 1] {
            b"lt" => b'<',
            b"gt" => b'>',
            b"amp" => b'&',
            b"quot" => b'"',
            b"apos" => b'\'',
            _ => return None,
        })
    }

    /// The bytes `d[a..b]` in UTF-8, as expat hands them to a handler.
    fn utf8(&self, a: usize, b: usize) -> Vec<u8> {
        let d = &self.d[a..b];
        match self.enc {
            Enc::Utf8 => d.to_vec(),
            Enc::Latin1 => d
                .iter()
                .map(|&c| char::from(c))
                .collect::<String>()
                .into_bytes(),
        }
    }
}

/// `XML_ErrorString` of the errors labels reach.
pub(crate) const INVALID_TOKEN: &str = "not well-formed (invalid token)";
const TAG_MISMATCH: &str = "mismatched tag";
const DUPLICATE_ATTRIBUTE: &str = "duplicate attribute";
const JUNK_AFTER_DOC_ELEMENT: &str = "junk after document element";
const UNDEFINED_ENTITY: &str = "undefined entity";
const BAD_CHAR_REF: &str = "reference to invalid character number";
const MISPLACED_XML_PI: &str = "XML or text declaration not at start of entity";

/// An `XML_Parse` error: `XML_ErrorString` and the byte offset (`m_eventPtr`) up to which
/// `XML_GetCurrentLineNumber` counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct XmlError {
    pub msg: &'static str,
    pub at: usize,
}

/// The callbacks `htmllex.c` registers. `line` is `XML_GetCurrentLineNumber` during the
/// callback.
pub(crate) trait Handler {
    fn start_element(&mut self, name: &str, atts: &[(String, String)], line: usize);
    fn end_element(&mut self, name: &str, line: usize);
    fn character_data(&mut self, s: &[u8]);
}

/// `XML_CONTEXT_BYTES` and `INIT_BUFFER_SIZE` in expat's `xmlparse.c`.
const XML_CONTEXT_BYTES: usize = 1024;
const INIT_BUFFER_SIZE: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Processor {
    Content,
    Cdata,
    Epilog,
}

/// An expat parser between `XML_Parse` calls.
pub(crate) struct Parser {
    enc: Enc,
    /// Every byte given to `XML_Parse`: offsets into it are stream positions.
    data: Vec<u8>,
    /// `m_bufferPtr`: the first byte not yet tokenized.
    ptr: usize,
    /// `m_buffer`'s size and where in it the stream's byte `data.len()` (`m_bufferEnd`) and
    /// `ptr` (`m_bufferPtr`) are; `lim` is 0 before the first allocation.
    buf_ptr: usize,
    buf_end: usize,
    lim: usize,
    /// `m_partialTokenBytesBefore`.
    had: usize,
    /// The raw names of the open elements (`m_tagStack`), as stream ranges.
    tags: Vec<(usize, usize)>,
    processor: Processor,
    /// The line count up to `line_at.0` (`m_position`, `m_positionPtr`).
    line_at: (usize, usize),
}

impl Parser {
    pub(crate) fn new(enc: Enc) -> Self {
        Parser {
            enc,
            data: Vec::new(),
            ptr: 0,
            buf_ptr: 0,
            buf_end: 0,
            lim: 0,
            had: 0,
            tags: Vec::new(),
            processor: Processor::Content,
            line_at: (0, 1),
        }
    }

    /// `m_bufferPtr`: where tokenizing stopped, which is where `XML_ParseBuffer` leaves
    /// `m_eventPtr` and the line count after a parse.
    pub(crate) fn position(&self) -> usize {
        self.ptr
    }

    /// `XML_GetCurrentLineNumber` with `m_eventPtr` at stream offset `p`.
    pub(crate) fn line(&mut self, p: usize) -> usize {
        count_lines(&self.data, &mut self.line_at, p)
    }

    /// `XML_GetBuffer(len)`: makes room for `len` bytes after `m_bufferEnd`.
    fn get_buffer(&mut self, len: usize) {
        if self.lim != 0 && len <= self.lim - self.buf_end {
            return;
        }
        let keep = self.buf_ptr.min(XML_CONTEXT_BYTES);
        let needed = len + (self.buf_end - self.buf_ptr) + keep;
        if self.lim != 0 && needed <= self.lim {
            let off = self.buf_ptr - keep;
            self.buf_end -= off;
            self.buf_ptr -= off;
        } else {
            let mut size = if self.lim == 0 {
                INIT_BUFFER_SIZE
            } else {
                self.lim
            };
            loop {
                size *= 2;
                if size >= needed {
                    break;
                }
            }
            if self.lim == 0 {
                self.buf_end = 0;
                self.buf_ptr = 0;
            } else {
                self.buf_end = self.buf_end - self.buf_ptr + keep;
                self.buf_ptr = keep;
            }
            self.lim = size;
        }
    }

    /// `XML_Parse(parser, chunk, len, 0)`, calling `h` for each event.
    pub(crate) fn parse(&mut self, chunk: &[u8], h: &mut impl Handler) -> Result<(), XmlError> {
        let len = chunk.len();
        self.get_buffer(len);
        self.buf_end += len;
        self.data.extend_from_slice(chunk);
        // callProcessor: don't reparse a partial token until the bytes have doubled, unless
        // the buffer would have to grow
        let have = self.buf_end - self.buf_ptr;
        let available =
            self.buf_ptr - self.buf_ptr.min(XML_CONTEXT_BYTES) + (self.lim - self.buf_end);
        if have < 2 * self.had && len <= available {
            return Ok(());
        }
        let start = self.ptr;
        let r = self.run(h);
        self.buf_ptr = self.buf_end - (self.data.len() - self.ptr);
        if r.is_ok() {
            self.had = if self.ptr == start { have } else { 0 };
        }
        r
    }

    /// `callProcessor`'s loop: each processor hands over to the next within one parse.
    fn run(&mut self, h: &mut impl Handler) -> Result<(), XmlError> {
        loop {
            let p = self.processor;
            match p {
                Processor::Content => self.do_content(h)?,
                Processor::Cdata => {
                    if self.do_cdata_section(h)? {
                        self.processor = Processor::Content;
                    }
                }
                Processor::Epilog => self.epilog()?,
            }
            if self.processor == p {
                return Ok(());
            }
        }
    }

    fn err(msg: &'static str, at: usize) -> XmlError {
        XmlError { msg, at }
    }

    /// `doContent` until the bytes run out (or a CDATA section or the epilog starts).
    fn do_content(&mut self, h: &mut impl Handler) -> Result<(), XmlError> {
        let mut s = self.ptr;
        loop {
            let data = std::mem::take(&mut self.data);
            let tk = Tk {
                d: &data,
                enc: self.enc,
            };
            let tok = tk.content_tok(s);
            let r = self.content_token(&tk, tok, s, h);
            self.data = data;
            match r? {
                Some(next) => s = next,
                None => return Ok(()),
            }
        }
    }

    /// One token of `doContent`: the next token's start, or `None` once this parse is over.
    fn content_token(
        &mut self,
        tk: &Tk<'_>,
        tok: T,
        s: usize,
        h: &mut impl Handler,
    ) -> Result<Option<usize>, XmlError> {
        let next = match tok {
            // haveMore: htmllex never passes isFinal
            T::None | T::Partial | T::PartialChar | T::TrailingCr | T::TrailingRsqb => {
                self.ptr = s;
                return Ok(None);
            }
            T::Invalid(at) => return Err(Self::err(INVALID_TOKEN, at)),
            T::EntityRef(next) => {
                let Some(c) = tk.predefined_entity(s, next) else {
                    return Err(Self::err(UNDEFINED_ENTITY, s));
                };
                h.character_data(&[c]);
                next
            }
            T::StartTag(next) | T::EmptyElement(next) => {
                let name_end = s + 1 + tk.name_length(s + 1);
                let name = graphviz_pure_core::charset::decode(tk.utf8(s + 1, name_end));
                let atts = store_atts(tk, name_end, next, s)?;
                let line = self.line_with(tk, s);
                h.start_element(&name, &atts, line);
                if matches!(tok, T::StartTag(_)) {
                    self.tags.push((s + 1, name_end));
                } else {
                    let line = self.line_with(tk, next);
                    h.end_element(&name, line);
                    if self.tags.is_empty() {
                        self.ptr = next;
                        self.processor = Processor::Epilog;
                        return Ok(None);
                    }
                }
                next
            }
            T::EndTag(next) => {
                let raw = s + 2;
                let len = tk.name_length(raw);
                let Some(&(a, b)) = self.tags.last() else {
                    // XML_ERROR_ASYNC_ENTITY; the epilog takes end tags after the root
                    return Err(Self::err(INVALID_TOKEN, s));
                };
                if tk.d[a..b] != tk.d[raw..raw + len] {
                    return Err(Self::err(TAG_MISMATCH, raw));
                }
                self.tags.pop();
                let name = graphviz_pure_core::charset::decode(tk.utf8(a, b));
                let line = self.line_with(tk, s);
                h.end_element(&name, line);
                if self.tags.is_empty() {
                    self.ptr = next;
                    self.processor = Processor::Epilog;
                    return Ok(None);
                }
                next
            }
            T::CharRef(next) => {
                let Some(c) = tk.char_ref_number(s, next) else {
                    return Err(Self::err(BAD_CHAR_REF, s));
                };
                let mut b = [0; 4];
                h.character_data(c.encode_utf8(&mut b).as_bytes());
                next
            }
            T::XmlDecl(_) => return Err(Self::err(MISPLACED_XML_PI, s)),
            T::DataNewline(next) => {
                h.character_data(b"\n");
                next
            }
            T::CdataOpen(next) => {
                self.ptr = next;
                self.processor = Processor::Cdata;
                return Ok(None);
            }
            T::DataChars(next) => {
                h.character_data(&tk.utf8(s, next));
                next
            }
            T::Pi(next) | T::Comment(next) => next,
            // contentTok returns neither
            T::CdataClose(_) | T::Junk => return Err(Self::err(INVALID_TOKEN, s)),
        };
        Ok(Some(next))
    }

    /// [`Parser::line`] while `self.data` is lent to `tk`.
    fn line_with(&mut self, tk: &Tk<'_>, p: usize) -> usize {
        count_lines(tk.d, &mut self.line_at, p)
    }

    /// `doCdataSection`: whether the section closed in this parse.
    fn do_cdata_section(&mut self, h: &mut impl Handler) -> Result<bool, XmlError> {
        let data = std::mem::take(&mut self.data);
        let tk = Tk {
            d: &data,
            enc: self.enc,
        };
        let mut s = self.ptr;
        let r = loop {
            match tk.cdata_section_tok(s) {
                T::CdataClose(next) => {
                    s = next;
                    break Ok(true);
                }
                T::DataNewline(next) => {
                    h.character_data(b"\n");
                    s = next;
                }
                T::DataChars(next) => {
                    h.character_data(&tk.utf8(s, next));
                    s = next;
                }
                T::Invalid(at) => break Err(Self::err(INVALID_TOKEN, at)),
                _ => break Ok(false),
            }
        };
        self.data = data;
        self.ptr = s;
        r
    }

    /// `epilogProcessor`: after the root element only space, comments and PIs may follow.
    fn epilog(&mut self) -> Result<(), XmlError> {
        let tk = Tk {
            d: &self.data,
            enc: self.enc,
        };
        let mut s = self.ptr;
        loop {
            match tk.epilog_tok(s) {
                T::None | T::Partial | T::PartialChar => break,
                T::DataNewline(next) | T::Comment(next) | T::Pi(next) => s = next,
                T::Invalid(at) => return Err(Self::err(INVALID_TOKEN, at)),
                _ => return Err(Self::err(JUNK_AFTER_DOC_ELEMENT, s)),
            }
        }
        self.ptr = s;
        Ok(())
    }
}

/// `XmlUpdatePosition` from the last position counted (`m_positionPtr`) to `p`: LF, CR and
/// CR LF each end a line.
fn count_lines(d: &[u8], line_at: &mut (usize, usize), p: usize) -> usize {
    let (mut at, mut line) = *line_at;
    if p < at {
        (at, line) = (0, 1);
    }
    let d = &d[..p.min(d.len())];
    while at < d.len() {
        match d[at] {
            b'\n' => line += 1,
            b'\r' => {
                line += 1;
                if d.get(at + 1) == Some(&b'\n') {
                    at += 1;
                }
            }
            _ => {}
        }
        at += 1;
    }
    *line_at = (at, line);
    line
}

/// `storeAtts` for the start tag at `tag` whose name ends at `p` and which ends at `end`:
/// the attributes with values normalized by `appendAttributeValue`, in order.
fn store_atts(
    tk: &Tk<'_>,
    mut p: usize,
    end: usize,
    tag: usize,
) -> Result<Vec<(String, String)>, XmlError> {
    let mut atts: Vec<(String, String)> = Vec::new();
    let mut raw: Vec<&[u8]> = Vec::new();
    loop {
        while p < end && matches!(tk.bt(p), Bt::S | Bt::Cr | Bt::Lf) {
            p += 1;
        }
        if p >= end || matches!(tk.bt(p), Bt::Gt | Bt::Sol) {
            return Ok(atts);
        }
        let name = p;
        p += tk.name_length(p);
        let name_bytes = &tk.d[name..p];
        if raw.contains(&name_bytes) {
            return Err(Parser::err(DUPLICATE_ATTRIBUTE, name));
        }
        raw.push(name_bytes);
        while tk.d[p] != b'"' && tk.d[p] != b'\'' {
            p += 1;
        }
        let q = tk.d[p];
        p += 1;
        let vs = p;
        while tk.d[p] != q {
            p += 1;
        }
        let value = attribute_value(tk, vs, p, tag)?;
        p += 1;
        atts.push((
            graphviz_pure_core::charset::decode(tk.utf8(name, name + name_bytes.len())),
            graphviz_pure_core::charset::decode(value),
        ));
    }
}

/// `appendAttributeValue` for a CDATA attribute (no DTD declares otherwise): references
/// replaced, each space, tab, LF, CR or CR LF a space.
fn attribute_value(tk: &Tk<'_>, mut p: usize, end: usize, tag: usize) -> Result<Vec<u8>, XmlError> {
    let mut out = Vec::new();
    while p < end {
        match tk.bt(p) {
            Bt::Amp => {
                // scanAtts checked the reference
                let (T::EntityRef(next) | T::CharRef(next)) = tk.scan_ref(p + 1) else {
                    return Err(Parser::err(INVALID_TOKEN, p));
                };
                if tk.d[p + 1] == b'#' {
                    let Some(c) = tk.char_ref_number(p, next) else {
                        return Err(Parser::err(BAD_CHAR_REF, p));
                    };
                    let mut b = [0; 4];
                    out.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
                } else {
                    // an undefined entity leaves m_eventPtr at the tag
                    let c = tk
                        .predefined_entity(p, next)
                        .ok_or_else(|| Parser::err(UNDEFINED_ENTITY, tag))?;
                    out.push(c);
                }
                p = next;
            }
            Bt::Cr => {
                out.push(b' ');
                p += if tk.d.get(p + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
            }
            Bt::S | Bt::Lf => {
                out.push(b' ');
                p += 1;
            }
            _ => {
                let s = p;
                while p < end && !matches!(tk.bt(p), Bt::Amp | Bt::Cr | Bt::S | Bt::Lf) {
                    p += 1;
                }
                out.extend_from_slice(&tk.utf8(s, p));
            }
        }
    }
    Ok(out)
}

/// `namingBitmap` (`nametab.h`).
#[rustfmt::skip]
static NAMING_BITMAP: [u32; 320] = [
    0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF,
    0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0x00000000, 0x04000000,
    0x87FFFFFE, 0x07FFFFFE, 0x00000000, 0x00000000, 0xFF7FFFFF, 0xFF7FFFFF,
    0xFFFFFFFF, 0x7FF3FFFF, 0xFFFFFDFE, 0x7FFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF,
    0xFFFFE00F, 0xFC31FFFF, 0x00FFFFFF, 0x00000000, 0xFFFF0000, 0xFFFFFFFF,
    0xFFFFFFFF, 0xF80001FF, 0x00000003, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0xFFFFD740, 0xFFFFFFFB, 0x547F7FFF, 0x000FFFFD,
    0xFFFFDFFE, 0xFFFFFFFF, 0xDFFEFFFF, 0xFFFFFFFF, 0xFFFF0003, 0xFFFFFFFF,
    0xFFFF199F, 0x033FCFFF, 0x00000000, 0xFFFE0000, 0x027FFFFF, 0xFFFFFFFE,
    0x0000007F, 0x00000000, 0xFFFF0000, 0x000707FF, 0x00000000, 0x07FFFFFE,
    0x000007FE, 0xFFFE0000, 0xFFFFFFFF, 0x7CFFFFFF, 0x002F7FFF, 0x00000060,
    0xFFFFFFE0, 0x23FFFFFF, 0xFF000000, 0x00000003, 0xFFF99FE0, 0x03C5FDFF,
    0xB0000000, 0x00030003, 0xFFF987E0, 0x036DFDFF, 0x5E000000, 0x001C0000,
    0xFFFBAFE0, 0x23EDFDFF, 0x00000000, 0x00000001, 0xFFF99FE0, 0x23CDFDFF,
    0xB0000000, 0x00000003, 0xD63DC7E0, 0x03BFC718, 0x00000000, 0x00000000,
    0xFFFDDFE0, 0x03EFFDFF, 0x00000000, 0x00000003, 0xFFFDDFE0, 0x03EFFDFF,
    0x40000000, 0x00000003, 0xFFFDDFE0, 0x03FFFDFF, 0x00000000, 0x00000003,
    0x00000000, 0x00000000, 0x00000000, 0x00000000, 0xFFFFFFFE, 0x000D7FFF,
    0x0000003F, 0x00000000, 0xFEF02596, 0x200D6CAE, 0x0000001F, 0x00000000,
    0x00000000, 0x00000000, 0xFFFFFEFF, 0x000003FF, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0xFFFFFFFF, 0xFFFF003F, 0x007FFFFF, 0x0007DAED, 0x50000000,
    0x82315001, 0x002C62AB, 0x40000000, 0xF580C900, 0x00000007, 0x02010800,
    0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0x0FFFFFFF, 0xFFFFFFFF,
    0xFFFFFFFF, 0x03FFFFFF, 0x3F3FFFFF, 0xFFFFFFFF, 0xAAFF3F3F, 0x3FFFFFFF,
    0xFFFFFFFF, 0x5FDFFFFF, 0x0FCF1FDC, 0x1FDC1FFF, 0x00000000, 0x00004C40,
    0x00000000, 0x00000000, 0x00000007, 0x00000000, 0x00000000, 0x00000000,
    0x00000080, 0x000003FE, 0xFFFFFFFE, 0xFFFFFFFF, 0x001FFFFF, 0xFFFFFFFE,
    0xFFFFFFFF, 0x07FFFFFF, 0xFFFFFFE0, 0x00001FFF, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000, 0xFFFFFFFF, 0xFFFFFFFF,
    0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0x0000003F, 0x00000000, 0x00000000,
    0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0x0000000F,
    0x00000000, 0x00000000, 0x00000000, 0x07FF6000, 0x87FFFFFE, 0x07FFFFFE,
    0x00000000, 0x00800000, 0xFF7FFFFF, 0xFF7FFFFF, 0x00FFFFFF, 0x00000000,
    0xFFFF0000, 0xFFFFFFFF, 0xFFFFFFFF, 0xF80001FF, 0x00030003, 0x00000000,
    0xFFFFFFFF, 0xFFFFFFFF, 0x0000003F, 0x00000003, 0xFFFFD7C0, 0xFFFFFFFB,
    0x547F7FFF, 0x000FFFFD, 0xFFFFDFFE, 0xFFFFFFFF, 0xDFFEFFFF, 0xFFFFFFFF,
    0xFFFF007B, 0xFFFFFFFF, 0xFFFF199F, 0x033FCFFF, 0x00000000, 0xFFFE0000,
    0x027FFFFF, 0xFFFFFFFE, 0xFFFE007F, 0xBBFFFFFB, 0xFFFF0016, 0x000707FF,
    0x00000000, 0x07FFFFFE, 0x0007FFFF, 0xFFFF03FF, 0xFFFFFFFF, 0x7CFFFFFF,
    0xFFEF7FFF, 0x03FF3DFF, 0xFFFFFFEE, 0xF3FFFFFF, 0xFF1E3FFF, 0x0000FFCF,
    0xFFF99FEE, 0xD3C5FDFF, 0xB080399F, 0x0003FFCF, 0xFFF987E4, 0xD36DFDFF,
    0x5E003987, 0x001FFFC0, 0xFFFBAFEE, 0xF3EDFDFF, 0x00003BBF, 0x0000FFC1,
    0xFFF99FEE, 0xF3CDFDFF, 0xB0C0398F, 0x0000FFC3, 0xD63DC7EC, 0xC3BFC718,
    0x00803DC7, 0x0000FF80, 0xFFFDDFEE, 0xC3EFFDFF, 0x00603DDF, 0x0000FFC3,
    0xFFFDDFEC, 0xC3EFFDFF, 0x40603DDF, 0x0000FFC3, 0xFFFDDFEC, 0xC3FFFDFF,
    0x00803DCF, 0x0000FFC3, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0xFFFFFFFE, 0x07FF7FFF, 0x03FF7FFF, 0x00000000, 0xFEF02596, 0x3BFF6CAE,
    0x03FF3F5F, 0x00000000, 0x03000000, 0xC2A003FF, 0xFFFFFEFF, 0xFFFE03FF,
    0xFEBF0FDF, 0x02FE3FFF, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x1FFF0000, 0x00000002,
    0x000000A0, 0x003EFFFE, 0xFFFFFFFE, 0xFFFFFFFF, 0x661FFFFF, 0xFFFFFFFE,
    0xFFFFFFFF, 0x77FFFFFF,
];

/// `nmstrtPages` (`nametab.h`).
#[rustfmt::skip]
static NMSTRT_PAGES: [u8; 256] = [
    0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x00, 0x00, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
    0x10, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x13,
    0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x15, 0x16, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x17,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// `namePages` (`nametab.h`).
#[rustfmt::skip]
static NAME_PAGES: [u8; 256] = [
    0x19, 0x03, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x00, 0x00, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25,
    0x10, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x13,
    0x26, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x27, 0x16, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x17,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
