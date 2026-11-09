//! The interface's bitmap fonts, `.fnt` files (docs/spec-notes/fonts.md).
//!
//! A `.fnt` file is a table of 256 glyphs, one per character code, over one texture that
//! holds their pictures (§2). [`Font::parse`] reads it; [`Font::lines`] sets a text in
//! lines, wrapping at a width (§3). Lengths are in the interface's units, which are the
//! texture's texels at the font's own size.

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FontError {
    #[error("font file is {found} bytes, expected {expected}")]
    Size { found: usize, expected: usize },
    #[error("font names {0} textures; only fonts with one are understood")]
    Textures(u32),
    #[error("font has no texture name")]
    NoTexture,
}

/// Bytes before the glyph table: the size, two counts and the texture name's room (§2).
const HEADER: usize = 12;
const NAME_ROOM: usize = 284;
const GLYPH: usize = 56;
/// The size of every font file (§2).
pub const FILE_SIZE: usize = HEADER + NAME_ROOM + 256 * GLYPH;

/// A font's picture as a `.tex` file (docs/spec-notes/fonts.md §2.2): the width and the
/// height as little-endian u32, then four bytes a texel, rows from the top, the colour in
/// the first three bytes and the alpha in the fourth [observed: the English and Russian
/// installs' font `.tex` files against their `.dds`].
#[derive(Debug, Clone, PartialEq)]
pub struct Tex {
    pub width: u32,
    pub height: u32,
    /// `width * height` texels, four bytes each: red, green, blue, alpha.
    pub rgba: Vec<u8>,
}

/// Bytes before a `.tex` file's texels.
const TEX_HEADER: usize = 8;

/// The largest side a `.tex` is read with (512 in every file seen; a larger one is a
/// broken header).
pub const TEX_MAX_SIDE: u32 = 8192;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TexError {
    #[error(".tex file is {0} bytes, shorter than its header")]
    Short(usize),
    #[error(".tex size {0}x{1} out of range (1 to {TEX_MAX_SIDE})")]
    Side(u32, u32),
    #[error(".tex file is {found} bytes, expected {expected} for its size")]
    Size { found: usize, expected: usize },
}

impl Tex {
    pub fn parse(bytes: &[u8]) -> Result<Tex, TexError> {
        if bytes.len() < TEX_HEADER {
            return Err(TexError::Short(bytes.len()));
        }
        let (width, height) = (u32_at(bytes, 0), u32_at(bytes, 4));
        if width == 0 || height == 0 || width > TEX_MAX_SIDE || height > TEX_MAX_SIDE {
            return Err(TexError::Side(width, height));
        }
        let expected = TEX_HEADER + width as usize * height as usize * 4;
        if bytes.len() != expected {
            return Err(TexError::Size { found: bytes.len(), expected });
        }
        Ok(Tex { width, height, rgba: bytes[TEX_HEADER..].to_vec() })
    }
}

/// One character's picture and measures (§2).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Glyph {
    /// The picture's corners on the texture, each 0 to 1: left, top, right, bottom.
    pub uv: [f32; 4],
    pub width: f32,
    pub height: f32,
    /// Empty room before and after the picture; the next character starts at
    /// `before + width + after` [observed, §3].
    pub before: f32,
    pub after: f32,
    /// How far the picture's top is above the line the letters stand on.
    pub ascent: f32,
}

impl Glyph {
    pub fn advance(&self) -> f32 {
        self.before + self.width + self.after
    }

    /// Whether there is a picture to draw (a space has none).
    pub fn drawn(&self) -> bool {
        self.width > 0.0 && self.height > 0.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// The first number of the file: the height of one line of text (§2).
    pub size: f32,
    /// The texture's name as the file has it, without folder or extension.
    pub texture: String,
    /// By character code, 256 of them.
    pub glyphs: Vec<Glyph>,
}

/// A character set in a line: its code and where its glyph's room starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub code: u8,
    pub x: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Line {
    pub glyphs: Vec<Placed>,
    /// From the line's start to the end of its last character.
    pub width: f32,
}

impl Line {
    /// The line's characters, its codes read back in this run's code page (the spaces it
    /// was broken at are not in it).
    pub fn text(&self) -> String {
        self.text_in(fnv_util::text::code_page())
    }

    /// The line's characters, its codes read back in a given code page.
    pub fn text_in(&self, page: fnv_util::text::CodePage) -> String {
        self.glyphs.iter().map(|g| page.char_of(g.code)).collect()
    }
}

fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// The code of a character in the game's text, in this run's code page
/// (`fnv_util::text::code_page`, Windows-1252 unless set otherwise); `?` for one it lacks.
pub fn code_of(c: char) -> u8 {
    code_in(c, fnv_util::text::code_page())
}

/// The code of a character in a given code page; `?` for one it lacks.
pub fn code_in(c: char, page: fnv_util::text::CodePage) -> u8 {
    page.code_of(c).unwrap_or(b'?')
}

impl Font {
    pub fn parse(bytes: &[u8]) -> Result<Font, FontError> {
        if bytes.len() != FILE_SIZE {
            return Err(FontError::Size { found: bytes.len(), expected: FILE_SIZE });
        }
        let textures = u32_at(bytes, 4);
        if textures != 1 {
            return Err(FontError::Textures(textures));
        }
        let name = &bytes[HEADER..HEADER + NAME_ROOM];
        let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        if end == 0 {
            return Err(FontError::NoTexture);
        }
        let texture = name[..end].iter().map(|&b| b as char).collect();
        let glyphs = (0..256)
            .map(|code| {
                let at = HEADER + NAME_ROOM + code * GLYPH;
                let f = |k: usize| f32_at(bytes, at + 4 * k);
                // Numbers 1 to 8 are the four corners; the top left and bottom right say it all.
                Glyph { uv: [f(1), f(2), f(7), f(8)], width: f(9), height: f(10), before: f(11), after: f(12), ascent: f(13) }
            })
            .collect();
        Ok(Font { size: f32_at(bytes, 0), texture, glyphs })
    }

    /// The texture's path in the game's files.
    pub fn texture_path(&self) -> String {
        format!("textures\\fonts\\{}.dds", self.texture.to_ascii_lowercase())
    }

    /// The same picture as a `.tex` file ([`Tex`], fonts.md §2.2): the one whose letters
    /// the glyph table describes, where a localized install left the English `.dds` beside it.
    pub fn tex_path(&self) -> String {
        format!("textures\\fonts\\{}.tex", self.texture.to_ascii_lowercase())
    }

    pub fn glyph(&self, code: u8) -> &Glyph {
        &self.glyphs[code as usize]
    }

    /// The width of a text set in one line.
    pub fn width(&self, text: &str) -> f32 {
        text.chars().map(|c| self.glyph(code_of(c)).advance()).sum()
    }

    /// Set a text in lines (§3): a line ends at a line break, and before the word that
    /// would reach past `wrap`. The spaces a line is broken at are not set. A word wider
    /// than `wrap` gets a line of its own and is not cut.
    pub fn lines(&self, text: &str, wrap: Option<f32>) -> Vec<Line> {
        let mut out = Vec::new();
        for para in text.replace("\r\n", "\n").split('\n') {
            let mut line = Line::default();
            // Spaces met since the line's last word; set only if a word follows them.
            let mut gap = 0.0;
            for (i, word) in para.split(' ').enumerate() {
                if i > 0 {
                    gap += self.glyph(b' ').advance();
                }
                if word.is_empty() {
                    continue;
                }
                let w = self.width(word);
                if wrap.is_some_and(|max| !line.glyphs.is_empty() && line.width + gap + w > max) {
                    out.push(std::mem::take(&mut line));
                    gap = 0.0;
                }
                let mut x = line.width + gap;
                for c in word.chars() {
                    let code = code_of(c);
                    line.glyphs.push(Placed { code, x });
                    x += self.glyph(code).advance();
                }
                line.width = x;
                gap = 0.0;
            }
            out.push(line);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A font file where every letter is 10 wide with 1 after it and a space is 5.
    fn file() -> Vec<u8> {
        let mut b = vec![0u8; FILE_SIZE];
        b[0..4].copy_from_slice(&31.0f32.to_le_bytes());
        b[4..8].copy_from_slice(&1u32.to_le_bytes());
        b[8..12].copy_from_slice(&1u32.to_le_bytes());
        b[12..20].copy_from_slice(b"Test_Lod");
        for code in 0..256usize {
            let at = HEADER + NAME_ROOM + code * GLYPH;
            let mut put = |k: usize, v: f32| b[at + 4 * k..at + 4 * k + 4].copy_from_slice(&v.to_le_bytes());
            if code == b' ' as usize {
                put(12, 5.0);
            } else if code > 32 && code < 127 {
                for (k, v) in [(1, 0.25), (2, 0.5), (3, 0.3), (4, 0.5), (5, 0.25), (6, 0.6), (7, 0.3), (8, 0.6), (9, 10.0), (10, 26.0), (12, 1.0), (13, 24.0)] {
                    put(k, v);
                }
            }
        }
        b
    }

    #[test]
    fn a_file_is_read() {
        let f = Font::parse(&file()).unwrap();
        assert_eq!(f.size, 31.0);
        assert_eq!(f.texture, "Test_Lod");
        assert_eq!(f.texture_path(), "textures\\fonts\\test_lod.dds");
        assert_eq!(f.tex_path(), "textures\\fonts\\test_lod.tex");
        let a = f.glyph(b'A');
        assert_eq!(a.uv, [0.25, 0.5, 0.3, 0.6]);
        assert_eq!((a.width, a.height, a.after, a.ascent), (10.0, 26.0, 1.0, 24.0));
        assert_eq!(a.advance(), 11.0);
        assert!(a.drawn() && !f.glyph(b' ').drawn());
        assert_eq!(f.glyph(b' ').advance(), 5.0);
    }

    #[test]
    fn a_wrong_file_is_refused() {
        assert_eq!(Font::parse(&[0; 100]), Err(FontError::Size { found: 100, expected: FILE_SIZE }));
        let mut b = file();
        b[4] = 2;
        assert_eq!(Font::parse(&b), Err(FontError::Textures(2)));
        let mut b = file();
        b[12] = 0;
        assert_eq!(Font::parse(&b), Err(FontError::NoTexture));
    }

    #[test]
    fn a_tex_file_is_read() {
        let mut b = Vec::new();
        b.extend(2u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend([255, 255, 255, 200, 0, 0, 0, 7]);
        let t = Tex::parse(&b).unwrap();
        assert_eq!((t.width, t.height), (2, 1));
        assert_eq!(t.rgba, [255, 255, 255, 200, 0, 0, 0, 7]);
        assert_eq!(Tex::parse(&b[..5]), Err(TexError::Short(5)));
        assert_eq!(Tex::parse(&b[..12]), Err(TexError::Size { found: 12, expected: 16 }));
        b[0..4].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(Tex::parse(&b), Err(TexError::Side(0, 1)));
        b[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(Tex::parse(&b), Err(TexError::Side(u32::MAX, 1)));
    }

    #[test]
    fn lines_wrap_at_words() {
        let f = Font::parse(&file()).unwrap();
        assert_eq!(f.width("ab cd"), 49.0);
        let texts = |wrap: Option<f32>, s: &str| -> Vec<(String, f32)> {
            f.lines(s, wrap).iter().map(|l| (l.glyphs.iter().map(|g| g.code as char).collect(), l.width)).collect()
        };
        assert_eq!(texts(None, "ab cd"), vec![("abcd".to_string(), 49.0)]);
        // "ab cd" is 49 wide: it fits in 49 and not in 48.
        assert_eq!(texts(Some(49.0), "ab cd ef"), vec![("abcd".to_string(), 49.0), ("ef".to_string(), 22.0)]);
        assert_eq!(texts(Some(48.0), "ab cd"), vec![("ab".to_string(), 22.0), ("cd".to_string(), 22.0)]);
        // Line breaks are kept, an empty line too; a word too wide is not cut.
        assert_eq!(texts(Some(30.0), "ab\n\nabcdef g"), vec![("ab".to_string(), 22.0), (String::new(), 0.0), ("abcdef".to_string(), 66.0), ("g".to_string(), 11.0)]);
        // The second word of a line starts after the space.
        assert_eq!(f.lines("ab cd", None)[0].glyphs[2], Placed { code: b'c', x: 27.0 });
    }

    #[test]
    fn characters_have_codes() {
        assert_eq!(code_of('A'), 0x41);
        assert_eq!(code_of('é'), 0xE9);
        assert_eq!(code_of('\u{2019}'), 0x92);
        assert_eq!(code_of('\u{4E2D}'), b'?');
    }

    #[test]
    fn codes_follow_the_code_page() {
        use fnv_util::text::CodePage::*;
        for page in [Windows1252, Windows1251, Windows1250] {
            for b in 0u8..=255 {
                assert_eq!(code_in(page.char_of(b), page), b, "{page} {b:#04x}");
            }
        }
        assert_eq!(code_in('Ж', Windows1251), 0xC6);
        assert_eq!(code_in('Ж', Windows1252), b'?');
        assert_eq!(code_in('ż', Windows1250), 0xBF);
        assert_eq!(code_in('é', Windows1252), 0xE9);
        assert_eq!(code_in('\u{2019}', Windows1251), 0x92);
    }

    #[test]
    fn a_line_reads_back_in_its_code_page() {
        use fnv_util::text::CodePage::*;
        let line = |s: &str, page| Line { glyphs: s.chars().map(|c| Placed { code: code_in(c, page), x: 0.0 }).collect(), width: 0.0 };
        assert_eq!(line("Привет", Windows1251).text_in(Windows1251), "Привет");
        assert_eq!(line("Łódź", Windows1250).text_in(Windows1250), "Łódź");
        assert_eq!(line("café", Windows1252).text_in(Windows1252), "café");
        // Read as Latin-1 the same codes were other letters (the old stand-in text).
        assert_ne!(line("Привет", Windows1251).text_in(Windows1252), "Привет");
    }
}
