use std::fmt;
use std::fmt::Write;

use fixed_decimal::FixedDecimal;
use icu_decimal::FixedDecimalFormatter;
use icu_locid::{locale, Locale};
use num_traits::Zero;
use once_cell::sync::Lazy;
use writeable::Writeable;

use crate::Zahl;

pub struct Stück(pub Zahl);
impl fmt::Display for Stück {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let vor = self.0.trunc();
        let nach = self.0.fract();

        if !vor.is_zero() || nach.is_zero() {
            // TODO: number format
            write!(f, "{vor}")?;
            if !nach.is_zero() {
                f.write_char(' ')?;
            }
        }
        if !nach.is_zero() {
            write!(f, "{nach}")?;
        }
        Ok(())
    }
}

pub struct Eur(pub Zahl, pub u32);
impl fmt::Display for Eur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("€ ")?;

        let faktor = 10_i64.pow(self.1);
        let zahl = (self.0 * faktor).round().to_integer();

        let num = FixedDecimal::from(zahl).multiplied_pow10(-(self.1 as i16));

        DECIMAL_FORMATTER.format(&num).write_to(f)
    }
}

const LOCALE: Locale = locale!("de-AT");

mod provider {
    // icu4x-datagen --keys-for-bin ... --locales de-at --format=mod --use-separate-crates --pretty --overwrite
    include!("../icu4x_data/mod.rs");
}

static DECIMAL_FORMATTER: Lazy<FixedDecimalFormatter> = Lazy::new(|| {
    FixedDecimalFormatter::try_new_unstable(
        &provider::BakedDataProvider,
        &LOCALE.into(),
        Default::default(),
    )
    .unwrap()
});
