//! Closed literal authoring for packaging, sharing the existing config tokenizer.

use std::collections::BTreeMap;

use super::{AuthConfigError, ConfigReader, ConfigToken, auth_config_error, config_tokens};

const MAX_CONFIG_BYTES: usize = 64 * 1024;
const MAX_LITERAL_DEPTH: usize = 16;

/// One statically captured configuration value. No JavaScript is evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralConfigValue {
    /// A quoted UTF-8 string decoded by the shared configuration tokenizer.
    Text(String),
    /// An object whose keys are unique after decoding.
    Object(BTreeMap<String, Self>),
    /// An ordered array containing only quoted strings.
    Strings(Vec<String>),
}

/// A closed packaging authoring file could not be read as literal data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticConfigError {
    detail: &'static str,
}

impl From<AuthConfigError> for StaticConfigError {
    fn from(error: AuthConfigError) -> Self {
        Self {
            detail: error.detail,
        }
    }
}

impl std::fmt::Display for StaticConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "KELD-CORE-035: invalid static packaging configuration: {}. Use one export default object with literal strings, objects and string arrays; remove expressions, imports, spreads, computed keys and duplicate fields.",
            self.detail
        )
    }
}

impl std::error::Error for StaticConfigError {}

/// Captures a complete, bounded `export default` literal object for packaging.
///
/// The shared authentication tokenizer and object reader also own this stricter
/// whole-file mode. Comments, grouping parentheses, trailing commas and `as const`
/// are permitted. Existing development/auth extraction remains unchanged.
/// Field names and semantics remain the consuming owner's responsibility.
///
/// # Errors
/// Rejects files larger than 64 KiB, excessive nesting, nonliteral values,
/// duplicated decoded keys, malformed syntax, or other executable statements.
pub fn literal_config_from_ts(
    source: &str,
) -> Result<BTreeMap<String, LiteralConfigValue>, StaticConfigError> {
    if source.len() > MAX_CONFIG_BYTES {
        return Err(auth_config_error("authoring file exceeds 64 KiB").into());
    }
    let tokens = config_tokens(source)?;
    let mut reader = ConfigReader {
        tokens: &tokens,
        position: 0,
    };
    reader.literal_word("export")?;
    reader.literal_word("default")?;
    let mut groups = 0;
    while reader.take_symbol('(') {
        groups += 1;
        if groups > MAX_LITERAL_DEPTH {
            return Err(auth_config_error("export grouping exceeds 16 levels").into());
        }
    }
    let root = reader.literal_object(0)?;
    for _ in 0..groups {
        reader.literal_const_assertion()?;
        reader.symbol(')')?;
    }
    reader.literal_const_assertion()?;
    reader.take_symbol(';');
    if reader.position != tokens.len() {
        return Err(auth_config_error("tokens follow the complete exported object").into());
    }
    Ok(root)
}

impl ConfigReader<'_> {
    fn literal_word(&mut self, word: &str) -> Result<(), AuthConfigError> {
        if matches!(self.tokens.get(self.position), Some(ConfigToken::Word(found)) if found == word)
        {
            self.position += 1;
            Ok(())
        } else {
            Err(auth_config_error("expected a literal exported object"))
        }
    }

    fn literal_const_assertion(&mut self) -> Result<(), AuthConfigError> {
        if matches!(self.tokens.get(self.position), Some(ConfigToken::Word(word)) if word == "as") {
            self.position += 1;
            self.literal_word("const")?;
        }
        Ok(())
    }

    fn literal_object(
        &mut self,
        depth: usize,
    ) -> Result<BTreeMap<String, LiteralConfigValue>, AuthConfigError> {
        if depth >= MAX_LITERAL_DEPTH {
            return Err(auth_config_error(
                "literal object nesting exceeds 16 levels",
            ));
        }
        self.symbol('{')?;
        let mut fields = BTreeMap::new();
        while !self.take_symbol('}') {
            let key = self.key()?;
            self.symbol(':')?;
            let value = self.literal_value(depth + 1)?;
            if fields.insert(key, value).is_some() {
                return Err(auth_config_error("duplicate literal configuration field"));
            }
            self.object_separator()?;
        }
        Ok(fields)
    }

    fn literal_value(&mut self, depth: usize) -> Result<LiteralConfigValue, AuthConfigError> {
        match self.tokens.get(self.position) {
            Some(ConfigToken::Text(_)) => self.text().map(LiteralConfigValue::Text),
            Some(ConfigToken::Symbol('{')) => {
                self.literal_object(depth).map(LiteralConfigValue::Object)
            }
            Some(ConfigToken::Symbol('[')) => {
                self.position += 1;
                let mut values = Vec::new();
                while !self.take_symbol(']') {
                    values.push(self.text()?);
                    if !self.take_symbol(',')
                        && self.tokens.get(self.position) != Some(&ConfigToken::Symbol(']'))
                    {
                        return Err(auth_config_error(
                            "string array requires commas and a closing bracket",
                        ));
                    }
                }
                Ok(LiteralConfigValue::Strings(values))
            }
            _ => Err(auth_config_error(
                "only literal strings, objects and string arrays are supported",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_exact_decoded_values_and_preserves_array_order() {
        let fields = literal_config_from_ts(r"/* prefix */ export default ({ name: 'A & B', resources: ['b', 'a',], bun: { path: '/tmp/bun', version: '1.4.4', }, } as const); // suffix").expect("closed config");
        assert_eq!(
            fields["name"],
            LiteralConfigValue::Text(String::from("A & B"))
        );
        assert_eq!(
            fields["resources"],
            LiteralConfigValue::Strings(vec![String::from("b"), String::from("a")])
        );
        assert!(matches!(&fields["bun"], LiteralConfigValue::Object(value) if value.len() == 2));
    }

    #[test]
    fn rejects_ambiguous_executable_and_duplicate_sources() {
        for source in [
            "export default { name: 'a', name: 'b' };",
            r"export default { name: 'a', '\u006eame': 'b' };",
            "export default { host: { path: 'a', path: 'b' } };",
            "export default { ...other };",
            "export default { ['name']: 'a' };",
            "export default { name };",
            "export default { get name() { return 'a'; } };",
            "export default { name: 'a' + 'b' };",
            "export default { name: `a` };",
            "export default { resources: ['a', ...other] };",
            "export default { resources: [1] };",
            "export default { name: true };",
            "export default { name: 'a' } || other;",
            "export default { name: 'a' }; export default { name: 'b' };",
            "import value from './elsewhere'; export default { name: 'a' };",
            "export default makeConfig({ name: 'a' });",
            "export default { name: 'a' }; sideEffect();",
            "export default { name: 'a' }; /* unfinished",
            "export default { name: 'a'",
        ] {
            assert!(literal_config_from_ts(source).is_err(), "accepted {source}");
        }
    }

    #[test]
    fn honors_byte_and_nesting_limits() {
        let prefix = "export default {};//";
        let at_limit = format!("{prefix}{}", "x".repeat(MAX_CONFIG_BYTES - prefix.len()));
        assert!(literal_config_from_ts(&at_limit).is_ok());
        assert!(literal_config_from_ts(&(at_limit + "x")).is_err());
        let nested = format!(
            "export default {}'value'{};",
            "{a:".repeat(MAX_LITERAL_DEPTH),
            "}".repeat(MAX_LITERAL_DEPTH)
        );
        assert!(literal_config_from_ts(&nested).is_ok());
        let too_deep = format!(
            "export default {}'value'{};",
            "{a:".repeat(MAX_LITERAL_DEPTH + 1),
            "}".repeat(MAX_LITERAL_DEPTH + 1)
        );
        assert!(literal_config_from_ts(&too_deep).is_err());
    }
}
