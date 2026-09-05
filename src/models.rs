use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) text: String,
    pub(crate) kinds: u8,
}

impl Candidate {
    pub(crate) fn appears_in(&self, filter: Filter) -> bool {
        filter.kind().map_or(true, |kind| self.kinds & kind != 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Filter {
    All,
    Word,
    Line,
    Path,
    Url,
    Hash,
    Quote,
}

impl Filter {
    pub(crate) fn from_key(key: char) -> Option<Self> {
        match key.to_ascii_lowercase() {
            'a' => Some(Self::All),
            'w' => Some(Self::Word),
            'l' => Some(Self::Line),
            'p' => Some(Self::Path),
            'u' => Some(Self::Url),
            'h' => Some(Self::Hash),
            'q' => Some(Self::Quote),
            _ => None,
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Word => "word",
            Self::Line => "line",
            Self::Path => "path",
            Self::Url => "url",
            Self::Hash => "hash",
            Self::Quote => "quote",
        }
    }
    pub(crate) fn kind(self) -> Option<u8> {
        match self {
            Self::All => None,
            Self::Word => Some(crate::KIND_WORD),
            Self::Line => Some(crate::KIND_LINE),
            Self::Path => Some(crate::KIND_PATH),
            Self::Url => Some(crate::KIND_URL),
            Self::Hash => Some(crate::KIND_HASH),
            Self::Quote => Some(crate::KIND_QUOTE),
        }
    }
}

impl FromStr for Filter {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "all" => Ok(Self::All),
            "word" => Ok(Self::Word),
            "line" => Ok(Self::Line),
            "path" => Ok(Self::Path),
            "url" => Ok(Self::Url),
            "hash" => Ok(Self::Hash),
            "quote" => Ok(Self::Quote),
            _ => Err(format!("unknown filter `{value}`")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    Tab,
    Space,
    Server,
}

impl Scope {
    pub(crate) fn next(self, skip_tab: bool) -> Self {
        match self {
            Self::Space if skip_tab => Self::Server,
            Self::Space => Self::Tab,
            Self::Tab => Self::Server,
            Self::Server => Self::Space,
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Tab => "tab",
            Self::Space => "space",
            Self::Server => "server",
        }
    }
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Tab => 0,
            Self::Space => 1,
            Self::Server => 2,
        }
    }
}

impl FromStr for Scope {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "tab" => Ok(Self::Tab),
            "space" | "workspace" => Ok(Self::Space),
            "server" => Ok(Self::Server),
            _ => Err(format!("unknown scope `{value}`")),
        }
    }
}
