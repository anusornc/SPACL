//! Datatype value space: the XSD numeric lattice as intervals over
//! the integers (#118).
//!
//! Every integer subtype of `xsd:integer` the corpus exercises has a
//! value space that is an interval of ℤ (e.g. `xsd:byte` is
//! [−128, 127], `xsd:nonNegativeInteger` is [0, ∞)), so intersection
//! and containment between them are exact interval operations. Real
//! (float/double/decimal) and lexical (string) datatypes stay outside
//! this lattice and answer `None`.

/// The XSD namespace every lattice member and built-in lives under.
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// A closed interval of ℤ; a `None` bound is unbounded on that side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumericInterval {
    pub min: Option<i128>,
    pub max: Option<i128>,
}

impl NumericInterval {
    #[must_use]
    pub const fn unbounded() -> Self {
        Self {
            min: None,
            max: None,
        }
    }

    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        Self {
            min: match (self.min, other.min) {
                (Some(left), Some(right)) => Some(left.max(right)),
                (bound, None) | (None, bound) => bound,
            },
            max: match (self.max, other.max) {
                (Some(left), Some(right)) => Some(left.min(right)),
                (bound, None) | (None, bound) => bound,
            },
        }
    }

    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        let min_ok = match (self.min, other.min) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(bound), Some(value)) => bound <= value,
        };
        let max_ok = match (self.max, other.max) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(bound), Some(value)) => value <= bound,
        };
        min_ok && max_ok
    }
}

/// The interval of ℤ an XSD integer subtype covers, or `None` for
/// IRIs outside the numeric lattice.
#[must_use]
pub fn integer_interval(datatype_iri: &str) -> Option<NumericInterval> {
    let name = datatype_iri.strip_prefix(XSD)?;
    let interval = match name {
        "integer" => NumericInterval::unbounded(),
        "nonNegativeInteger" => NumericInterval {
            min: Some(0),
            max: None,
        },
        "nonPositiveInteger" => NumericInterval {
            min: None,
            max: Some(0),
        },
        "positiveInteger" => NumericInterval {
            min: Some(1),
            max: None,
        },
        "negativeInteger" => NumericInterval {
            min: None,
            max: Some(-1),
        },
        "long" => NumericInterval {
            min: Some(-9_223_372_036_854_775_808),
            max: Some(9_223_372_036_854_775_807),
        },
        "int" => NumericInterval {
            min: Some(-2_147_483_648),
            max: Some(2_147_483_647),
        },
        "short" => NumericInterval {
            min: Some(-32_768),
            max: Some(32_767),
        },
        "byte" => NumericInterval {
            min: Some(-128),
            max: Some(127),
        },
        "unsignedLong" => NumericInterval {
            min: Some(0),
            max: Some(18_446_744_073_709_551_615),
        },
        "unsignedInt" => NumericInterval {
            min: Some(0),
            max: Some(4_294_967_295),
        },
        "unsignedShort" => NumericInterval {
            min: Some(0),
            max: Some(65_535),
        },
        "unsignedByte" => NumericInterval {
            min: Some(0),
            max: Some(255),
        },
        _ => return None,
    };
    Some(interval)
}

/// Whether the IRI names an XSD built-in datatype of the OWL 2 DL
/// datatype map — the vocabulary every interpretation declares as
/// instances of `rdfs:Datatype` (#118, W3C I5.8-011). The numeric
/// lattice plus the five core non-numeric members; XSD types outside
/// the DL map (date, time, token, …) stay out.
#[must_use]
pub fn is_builtin_datatype_iri(datatype_iri: &str) -> bool {
    integer_interval(datatype_iri).is_some()
        || matches!(
            datatype_iri.strip_prefix(XSD),
            Some("string" | "boolean" | "decimal" | "float" | "double" | "dateTime")
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interval(datatype: &str) -> NumericInterval {
        integer_interval(&format!("http://www.w3.org/2001/XMLSchema#{datatype}"))
            .expect("numeric datatype has an interval")
    }

    #[test]
    fn byte_fits_inside_short() {
        assert!(interval("short").contains(interval("byte")));
    }

    #[test]
    fn short_does_not_fit_inside_byte() {
        assert!(!interval("byte").contains(interval("short")));
    }

    #[test]
    fn short_intersect_unsigned_int_fits_unsigned_short() {
        let crossed = interval("short").intersect(interval("unsignedInt"));
        assert!(interval("unsignedShort").contains(crossed));
    }

    #[test]
    fn non_negative_intersect_non_positive_is_zero() {
        let zero = interval("nonNegativeInteger").intersect(interval("nonPositiveInteger"));
        assert_eq!(
            zero,
            NumericInterval {
                min: Some(0),
                max: Some(0)
            }
        );
        assert!(interval("short").contains(zero));
    }

    #[test]
    fn unbounded_contains_everything_and_only_itself() {
        let universe = NumericInterval::unbounded();
        assert!(universe.contains(interval("integer")));
        assert!(universe.contains(interval("unsignedLong")));
        assert!(!interval("unsignedLong").contains(universe));
    }

    #[test]
    fn builtin_vocabulary_covers_numeric_and_lexical_builtins() {
        assert!(is_builtin_datatype_iri(
            "http://www.w3.org/2001/XMLSchema#integer"
        ));
        assert!(is_builtin_datatype_iri(
            "http://www.w3.org/2001/XMLSchema#string"
        ));
        assert!(!is_builtin_datatype_iri("http://example.org/NotADatatype"));
    }
}
