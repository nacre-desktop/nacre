//! The launcher's bounded arithmetic grammar; no script or shell evaluation.
use serde::Serialize;

const OPERANDS: &str = "Use numbers, arithmetic operators or supported functions";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Calculation {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub error: String,
}
impl Calculation {
    fn failure(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            value: None,
            error: error.into(),
        }
    }
}

pub fn evaluate(source: &str) -> Calculation {
    // String.trim in the existing runtime also trims U+0085. Its regexp \s
    // inside an expression does not; keep those two operations distinct.
    let source = source.trim_matches(|c| crate::text::whitespace(c) || c == '\u{0085}');
    if source.is_empty() {
        return Calculation::failure("Type a calculation, for example (12 + 8) / 5");
    }
    let text: Vec<_> = source.encode_utf16().collect();
    if text.len() > 256 {
        return Calculation::failure("Keep calculations under 256 characters");
    }
    let mut parser = Parser {
        text,
        cursor: 0,
        depth: 0,
        operations: 0,
    };
    let value = match parser.expression() {
        Ok(value) => value,
        Err(error) => return Calculation::failure(error),
    };
    parser.skip();
    if parser.cursor != parser.text.len() {
        return Calculation::failure(format!(
            "Check the expression near {}",
            String::from_utf16_lossy(
                &parser.text[parser.cursor..(parser.cursor + 12).min(parser.text.len())]
            )
        ));
    }
    if !value.is_finite() {
        return Calculation::failure("The result is undefined or outside the numeric range");
    }
    Calculation {
        ok: true,
        value: Some(number_text(value)),
        error: String::new(),
    }
}

struct Parser {
    text: Vec<u16>,
    cursor: usize,
    depth: usize,
    operations: usize,
}
type Parsed = Result<f64, &'static str>;
impl Parser {
    fn skip(&mut self) {
        while self
            .text
            .get(self.cursor)
            .and_then(|c| char::from_u32(u32::from(*c)))
            .is_some_and(crate::text::whitespace)
        {
            self.cursor += 1;
        }
    }
    fn matches(&self, token: &str) -> bool {
        self.text
            .get(self.cursor..self.cursor + token.len())
            .is_some_and(|part| part.iter().copied().eq(token.bytes().map(u16::from)))
    }
    fn consume(&mut self, token: &str) -> bool {
        self.skip();
        if !self.matches(token) {
            return false;
        }
        self.cursor += token.len();
        true
    }
    fn operation(&mut self) -> Result<(), &'static str> {
        self.operations += 1;
        if self.operations > 256 {
            return Err("Calculation is too long");
        }
        Ok(())
    }
    fn expression(&mut self) -> Parsed {
        let mut value = self.product()?;
        loop {
            if self.consume("+") {
                value += self.product()?;
            } else if self.consume("-") {
                value -= self.product()?;
            } else {
                return Ok(value);
            }
            self.operation()?;
        }
    }
    fn product(&mut self) -> Parsed {
        let mut value = self.unary()?;
        loop {
            self.skip();
            if self.matches("**") {
                return Ok(value);
            }
            if self.consume("*") {
                value *= self.unary()?;
            } else if self.consume("/") {
                value /= self.unary()?;
            } else if self.consume("%") {
                value %= self.unary()?;
            } else {
                return Ok(value);
            }
            self.operation()?;
        }
    }
    fn unary(&mut self) -> Parsed {
        self.depth += 1;
        if self.depth > 32 {
            return Err("Too many nested operations");
        }
        let value = if self.consume("+") {
            self.unary()?
        } else if self.consume("-") {
            -self.unary()?
        } else {
            self.power()?
        };
        self.depth -= 1;
        Ok(value)
    }
    fn power(&mut self) -> Parsed {
        let value = self.primary()?;
        if self.consume("**") || self.consume("^") {
            let exponent = self.unary()?;
            // ECMAScript keeps its historical NaN rules where libm returns 1:
            // https://tc39.es/ecma262/#sec-numeric-types-number-exponentiate
            Ok(
                if exponent.is_nan() || (value.abs() == 1.0 && exponent.is_infinite()) {
                    f64::NAN
                } else {
                    value.powf(exponent)
                },
            )
        } else {
            Ok(value)
        }
    }
    fn digit(&self) -> bool {
        self.text
            .get(self.cursor)
            .is_some_and(|c| (48..=57).contains(c))
    }
    fn primary(&mut self) -> Parsed {
        self.skip();
        if self.consume("(") {
            let value = self.expression()?;
            if !self.consume(")") {
                return Err("Close the parenthesis");
            }
            return Ok(value);
        }
        let start = self.cursor;
        while self.digit() {
            self.cursor += 1;
        }
        let integer = self.cursor > start;
        if self.matches(".") {
            self.cursor += 1;
            while self.digit() {
                self.cursor += 1;
            }
        }
        if integer || (self.cursor > start + 1) {
            let end = self.cursor;
            if self.matches("e") || self.matches("E") {
                self.cursor += 1;
                if self.matches("+") || self.matches("-") {
                    self.cursor += 1;
                }
                let exponent = self.cursor;
                while self.digit() {
                    self.cursor += 1;
                }
                if self.cursor == exponent {
                    self.cursor = end;
                }
            }
            return String::from_utf16_lossy(&self.text[start..self.cursor])
                .parse()
                .map_err(|_| OPERANDS);
        }
        self.cursor = start;
        let letter = |c: u16| (65..=90).contains(&c) || (97..=122).contains(&c);
        if self.text.get(self.cursor).is_some_and(|c| letter(*c)) {
            self.cursor += 1;
            while self
                .text
                .get(self.cursor)
                .is_some_and(|c| letter(*c) || (48..=57).contains(c))
            {
                self.cursor += 1;
            }
            let name =
                String::from_utf16_lossy(&self.text[start..self.cursor]).to_ascii_lowercase();
            match name.as_str() {
                "pi" => return Ok(std::f64::consts::PI),
                "e" => return Ok(std::f64::consts::E),
                "sqrt" | "abs" | "sin" | "cos" | "tan" | "log" | "log10" | "floor" | "ceil"
                | "round" => (),
                _ => return Err("Unknown function or constant"),
            }
            if !self.consume("(") {
                return Err("Put the function's argument in parentheses");
            }
            let value = self.expression()?;
            if !self.consume(")") {
                return Err("Close the parenthesis");
            }
            return Ok(match name.as_str() {
                "sqrt" => value.sqrt(),
                "abs" => value.abs(),
                "sin" => value.sin(),
                "cos" => value.cos(),
                "tan" => value.tan(),
                "log" => value.ln(),
                "log10" => value.log10(),
                "floor" => value.floor(),
                "ceil" => value.ceil(),
                _ => {
                    let floor = value.floor();
                    let rounded = if value - floor >= 0.5 {
                        floor + 1.0
                    } else {
                        floor
                    };
                    if rounded == 0.0 {
                        rounded.copysign(value)
                    } else {
                        rounded
                    }
                }
            });
        }
        Err(OPERANDS)
    }
}

// Check exact decimal representability without rounding a decimal midpoint into
// a nearby binary value. Fourteen significant digits can have genuine ties.
fn exact_decimal(mut mantissa: u128, mut exponent: i32, value: f64) -> bool {
    if exponent.unsigned_abs() > 22 {
        return false;
    }
    if exponent < 0 {
        for _ in exponent..0 {
            if !mantissa.is_multiple_of(5) {
                return false;
            }
            mantissa /= 5;
        }
    } else {
        for _ in 0..exponent {
            let Some(next) = mantissa.checked_mul(5) else {
                return false;
            };
            mantissa = next;
        }
    }
    while mantissa.is_multiple_of(2) && mantissa != 0 {
        mantissa /= 2;
        exponent += 1;
    }
    mantissa <= (1_u128 << 53) && (mantissa as f64) * 2.0_f64.powi(exponent) == value
}

fn number_text(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let mut precise = format!("{value:.13e}");
    // Rust formats decimal ties to even; toPrecision(14) chooses the larger
    // magnitude. Only adjust exact ties, not nearby values rounded to 15 digits.
    let extra = format!("{:.14e}", value.abs());
    let (digits, exponent) = extra.split_once('e').expect("scientific notation");
    let mantissa: u128 = digits.replace('.', "").parse().expect("decimal digits");
    let exponent: i32 = exponent.parse().expect("decimal exponent");
    if mantissa % 10 == 5 && exact_decimal(mantissa, exponent - 14, value.abs()) {
        precise = format!(
            "{}{}e{}",
            if value.is_sign_negative() { "-" } else { "" },
            mantissa / 10 + 1,
            exponent - 13
        );
    }
    let rounded: f64 = precise.parse().expect("formatted finite number");
    if rounded == 0.0 {
        return "0".into();
    }
    let shortest = rounded.abs().to_string();
    let (body, exponent) = shortest
        .split_once('e')
        .map_or((shortest.as_str(), 0), |(a, b)| {
            (a, b.parse::<i32>().expect("decimal exponent"))
        });
    let point = body.find('.').unwrap_or(body.len());
    let digits = body.replace('.', "");
    let first = digits.find(|c| c != '0').expect("nonzero number");
    let exponent = exponent + point as i32 - first as i32 - 1;
    let digits = digits[first..].trim_end_matches('0');
    let sign = if rounded.is_sign_negative() { "-" } else { "" };
    if !(-6..21).contains(&exponent) {
        format!(
            "{sign}{}{}e{}{exponent}",
            &digits[..1],
            if digits.len() > 1 {
                format!(".{}", &digits[1..])
            } else {
                String::new()
            },
            if exponent >= 0 { "+" } else { "" }
        )
    } else if exponent < 0 {
        format!("{sign}0.{}{digits}", "0".repeat((-exponent - 1) as usize))
    } else {
        let point = exponent as usize + 1;
        if point >= digits.len() {
            format!("{sign}{digits}{}", "0".repeat(point - digits.len()))
        } else {
            format!("{sign}{}.{}", &digits[..point], &digits[point..])
        }
    }
}
