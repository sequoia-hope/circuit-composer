//! Arithmetic for recipes and interface rules: `(rail_v - led.vf_v) / current_a`, `0.7*supply`.
//!
//! Numbers may carry an SI suffix (`4.7k`, `100n`, `2M`; `m` is milli). Names are letters,
//! digits, `_` and `.`; `min`, `max`, `abs`, `floor` and `ceil` are functions. Comparisons give 1 or 0.

/// A value in a part's value field: `4.7k` is 4700, `100n` is 1e-7. None when it isn't a number.
pub fn parse_value(s: &str) -> Option<f64> {
    let s = s.trim();
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).unwrap_or(s.len());
    let (num, rest) = s.split_at(split);
    let v: f64 = num.parse().ok()?;
    let rest = rest.trim();
    // `4k7` is 4.7k.
    let (mult, tail) = match rest.chars().next() {
        None => return Some(v),
        Some(c) => (si(c)?, &rest[c.len_utf8()..]),
    };
    let tail = tail.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == 'Ω');
    let frac = if tail.is_empty() { 0.0 } else { format!("0.{tail}").parse::<f64>().ok()? };
    Some((v + frac) * mult)
}

fn si(c: char) -> Option<f64> {
    Some(match c {
        'p' => 1e-12,
        'n' => 1e-9,
        'u' | 'µ' => 1e-6,
        'm' => 1e-3,
        'k' | 'K' => 1e3,
        'M' => 1e6,
        'R' | 'Ω' | 'V' | 'A' | 'F' | 'H' => 1.0,
        _ => return None,
    })
}

/// A resistance or capacitance as a part's value is written: 4700 is `4.7k`, 27 is `27`.
pub fn format_value(v: f64) -> String {
    let (m, suffix) = [(1e6, "M"), (1e3, "k"), (1.0, ""), (1e-3, "m"), (1e-6, "u"), (1e-9, "n"), (1e-12, "p")].into_iter().find(|(m, _)| v.abs() >= *m * 0.999_999).unwrap_or((1e-12, "p"));
    let x = v / m;
    let s = format!("{:.3}", x);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s}{suffix}")
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Name(String),
    Op(char),
    Cmp(&'static str),
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let cs: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && cs.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let st = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                i += 1;
            }
            // An exponent: 1e-9.
            if i < cs.len() && cs[i] == 'e' && cs.get(i + 1).is_some_and(|d| d.is_ascii_digit() || *d == '-') {
                i += 2;
                while i < cs.len() && cs[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let mut v: f64 = cs[st..i].iter().collect::<String>().parse().map_err(|_| format!("{s}: bad number"))?;
            // An SI suffix, unless it starts a name (`2 max` is not 2 milli).
            if let Some(&c) = cs.get(i) {
                if let Some(m) = si(c).filter(|_| !cs.get(i + 1).is_some_and(|d| d.is_alphanumeric() || *d == '_')) {
                    v *= m;
                    i += 1;
                }
            }
            out.push(Tok::Num(v));
        } else if c.is_alphabetic() || c == '_' {
            let st = i;
            while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_' || cs[i] == '.') {
                i += 1;
            }
            out.push(Tok::Name(cs[st..i].iter().collect()));
        } else if "<>=!".contains(c) {
            let two = cs.get(i + 1) == Some(&'=');
            let op = match (c, two) {
                ('<', true) => "<=",
                ('>', true) => ">=",
                ('=', true) => "==",
                ('!', true) => "!=",
                ('<', false) => "<",
                ('>', false) => ">",
                _ => return Err(format!("{s}: unexpected {c}")),
            };
            i += if two { 2 } else { 1 };
            out.push(Tok::Cmp(op));
        } else if "+-*/(),".contains(c) {
            out.push(Tok::Op(c));
            i += 1;
        } else {
            return Err(format!("{s}: unexpected {c}"));
        }
    }
    Ok(out)
}

struct P<'a> {
    t: Vec<Tok>,
    i: usize,
    src: &'a str,
    var: &'a dyn Fn(&str) -> Option<f64>,
}

impl P<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn cmp(&mut self) -> Result<f64, String> {
        let a = self.sum()?;
        if let Some(Tok::Cmp(op)) = self.peek().cloned() {
            self.i += 1;
            let b = self.sum()?;
            let r = match op {
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                ">=" => a >= b,
                "==" => (a - b).abs() < 1e-12,
                _ => (a - b).abs() >= 1e-12,
            };
            return Ok(if r { 1.0 } else { 0.0 });
        }
        Ok(a)
    }
    fn sum(&mut self) -> Result<f64, String> {
        let mut a = self.product()?;
        while let Some(Tok::Op(c @ ('+' | '-'))) = self.peek().cloned() {
            self.i += 1;
            let b = self.product()?;
            a = if c == '+' { a + b } else { a - b };
        }
        Ok(a)
    }
    fn product(&mut self) -> Result<f64, String> {
        let mut a = self.unary()?;
        while let Some(Tok::Op(c @ ('*' | '/'))) = self.peek().cloned() {
            self.i += 1;
            let b = self.unary()?;
            a = if c == '*' { a * b } else { a / b };
        }
        Ok(a)
    }
    fn unary(&mut self) -> Result<f64, String> {
        if let Some(Tok::Op('-')) = self.peek() {
            self.i += 1;
            return Ok(-self.unary()?);
        }
        self.atom()
    }
    fn atom(&mut self) -> Result<f64, String> {
        let t = self.t.get(self.i).cloned().ok_or_else(|| format!("{}: ends early", self.src))?;
        self.i += 1;
        match t {
            Tok::Num(v) => Ok(v),
            Tok::Op('(') => {
                let v = self.cmp()?;
                self.expect(')')?;
                Ok(v)
            }
            Tok::Name(n) if matches!(self.peek(), Some(Tok::Op('('))) => {
                self.i += 1;
                let mut args = vec![self.cmp()?];
                while let Some(Tok::Op(',')) = self.peek() {
                    self.i += 1;
                    args.push(self.cmp()?);
                }
                self.expect(')')?;
                match (n.as_str(), &args[..]) {
                    ("min", [_, ..]) => Ok(args.iter().cloned().fold(f64::INFINITY, f64::min)),
                    ("max", [_, ..]) => Ok(args.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
                    ("abs", [x]) => Ok(x.abs()),
                    ("floor", [x]) => Ok(x.floor()),
                    ("ceil", [x]) => Ok(x.ceil()),
                    _ => Err(format!("{}: no function {n} of {} arguments", self.src, args.len())),
                }
            }
            Tok::Name(n) => (self.var)(&n).ok_or_else(|| format!("{}: {n} is not known here", self.src)),
            t => Err(format!("{}: unexpected {t:?}", self.src)),
        }
    }
    fn expect(&mut self, c: char) -> Result<(), String> {
        match self.t.get(self.i) {
            Some(Tok::Op(x)) if *x == c => {
                self.i += 1;
                Ok(())
            }
            _ => Err(format!("{}: expected {c}", self.src)),
        }
    }
}

/// Evaluate `s` with names looked up by `var`.
pub fn eval(s: &str, var: &dyn Fn(&str) -> Option<f64>) -> Result<f64, String> {
    let mut p = P { t: lex(s)?, i: 0, src: s, var };
    let v = p.cmp()?;
    if p.i != p.t.len() {
        return Err(format!("{s}: unexpected {:?}", p.t[p.i]));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_names_and_suffixes() {
        let var = |n: &str| match n {
            "rail_v" => Some(3.3),
            "led.vf_v" => Some(2.0),
            _ => None,
        };
        assert!((eval("(rail_v - led.vf_v) / 2m", &var).unwrap() - 650.0).abs() < 1e-9);
        assert_eq!(eval("max(1, 2k, 3) + min(4, -5)", &var).unwrap(), 1995.0);
        assert_eq!(eval("2 < 3", &var).unwrap(), 1.0);
        assert!((eval("300n / (0.8473 * 50p)", &var).unwrap() - 7081.3).abs() < 0.1);
        assert!(eval("nope + 1", &var).is_err());
    }

    #[test]
    fn values() {
        assert_eq!(parse_value("4.7k"), Some(4700.0));
        assert_eq!(parse_value("4k7"), Some(4700.0));
        assert!((parse_value("100n").unwrap() - 1e-7).abs() < 1e-20);
        assert_eq!(parse_value("27"), Some(27.0));
        assert_eq!(parse_value("1M"), Some(1e6));
        assert_eq!(parse_value("GREEN"), None);
        assert_eq!(format_value(4700.0), "4.7k");
        assert_eq!(format_value(27.0), "27");
        assert_eq!(format_value(1e6), "1M");
        assert_eq!(format_value(5100.0), "5.1k");
    }
}
