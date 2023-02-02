// SPDX-FileCopyrightText: 2023 David Runge <dave@sleepmap.de>
// SPDX-License-Identifier: Apache-2.0 OR MIT

/// A convenient way to create a regular expression only once
///
/// A string literal as input is used to define the regular expression.
/// With the help of OnceCell the regular expression is created only once.
///
/// ## Examples
/// ```
/// #[macro_use] extern crate alpm_types;
///
/// let re = regex_once!("^(foo)$");
/// assert!(re.is_match("foo"));
/// ```
macro_rules! regex_once {
    ($re:literal $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
    ($re:ident $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new(&$re).unwrap())
    }};
}

pub(crate) use regex_once;

#[cfg(test)]
mod tests {

    use super::*;
    use rstest::rstest;

    #[rstest]
    fn test_regex_once_literal() {
        assert!(regex_once!("^foo$").is_match("foo"));
    }

    #[rstest]
    fn test_regex_once_ident() {
        let regex_string = "^foo$";
        assert!(regex_once!(regex_string).is_match("foo"));
    }
}
