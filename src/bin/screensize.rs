use anyhow::{bail, Context, Result};
use clap::Parser;

const CM_PER_INCH: f64 = 2.54;

#[derive(Parser, Debug)]
#[command(name = "screensize")]
#[command(about = "Calculate screen dimensions from one measurement and an aspect ratio")]
struct Args {
    /// Measurement (<value><unit><kind>) and ratio (W:H) in any order, e.g. 7id 16:9
    first: String,
    /// Measurement (<value><unit><kind>) and ratio (W:H) in any order, e.g. 16:9 7id
    second: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Inches,
    Centimeters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MeasurementKind {
    Diagonal,
    Width,
    Height,
}

#[derive(Debug, Clone, Copy)]
struct ScreenSize {
    diagonal_in: f64,
    width_in: f64,
    height_in: f64,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let ((value, unit, kind), ratio, ratio_text) = parse_inputs(&args.first, &args.second)?;
    let value_in = to_inches(value, unit);
    let screen = calculate_screen_size(value_in, kind, ratio.0, ratio.1)?;

    println!("ratio: {ratio_text}");
    println!("diagonal: {}", format_dual_unit(screen.diagonal_in));
    println!("width: {}", format_dual_unit(screen.width_in));
    println!("height: {}", format_dual_unit(screen.height_in));

    Ok(())
}

fn parse_inputs(
    first: &str,
    second: &str,
) -> Result<((f64, Unit, MeasurementKind), (f64, f64), String)> {
    if let Ok(ratio) = parse_ratio(first) {
        let measurement =
            parse_measurement(second).with_context(|| "Expected measurement as the other argument")?;
        return Ok((measurement, ratio, first.to_string()));
    }

    if let Ok(ratio) = parse_ratio(second) {
        let measurement =
            parse_measurement(first).with_context(|| "Expected measurement as the other argument")?;
        return Ok((measurement, ratio, second.to_string()));
    }

    bail!(
        "Expected one measurement (<value><unit><kind>) and one ratio (W:H), got '{}' and '{}'",
        first,
        second
    );
}

fn parse_ratio(input: &str) -> Result<(f64, f64)> {
    let (left, right) = input
        .split_once(':')
        .with_context(|| format!("Invalid ratio '{input}', expected format W:H"))?;

    let ratio_w: f64 = left
        .parse()
        .with_context(|| format!("Invalid ratio width '{left}' in '{input}'"))?;
    let ratio_h: f64 = right
        .parse()
        .with_context(|| format!("Invalid ratio height '{right}' in '{input}'"))?;

    if ratio_w <= 0.0 || ratio_h <= 0.0 {
        bail!("Ratio values must be greater than zero");
    }

    Ok((ratio_w, ratio_h))
}

fn parse_measurement(input: &str) -> Result<(f64, Unit, MeasurementKind)> {
    if input.len() < 3 {
        bail!("Invalid measurement '{input}', expected format <value><unit><kind>");
    }

    let kind_char = input
        .chars()
        .last()
        .with_context(|| format!("Invalid measurement '{input}'"))?;
    let kind = match kind_char {
        'd' => MeasurementKind::Diagonal,
        'w' => MeasurementKind::Width,
        'h' => MeasurementKind::Height,
        _ => bail!("Invalid kind '{kind_char}' in '{input}', expected d, w, or h"),
    };

    let prefix = &input[..input.len() - 1];
    let (value_part, unit) = if let Some(value) = prefix.strip_suffix("cm") {
        (value, Unit::Centimeters)
    } else if let Some(value) = prefix.strip_suffix('i') {
        (value, Unit::Inches)
    } else {
        bail!("Invalid unit in '{input}', expected i or cm before d/w/h");
    };

    if value_part.is_empty() {
        bail!("Missing numeric value in '{input}'");
    }

    let value: f64 = value_part
        .parse()
        .with_context(|| format!("Invalid numeric value '{value_part}' in '{input}'"))?;

    if value <= 0.0 {
        bail!("Measurement value must be greater than zero");
    }

    Ok((value, unit, kind))
}

fn calculate_screen_size(
    measured_in: f64,
    kind: MeasurementKind,
    ratio_w: f64,
    ratio_h: f64,
) -> Result<ScreenSize> {
    let ratio_diag = (ratio_w * ratio_w + ratio_h * ratio_h).sqrt();

    let (width_in, height_in, diagonal_in) = match kind {
        MeasurementKind::Diagonal => {
            let scale = measured_in / ratio_diag;
            let width = ratio_w * scale;
            let height = ratio_h * scale;
            (width, height, measured_in)
        }
        MeasurementKind::Width => {
            let width = measured_in;
            let height = width * ratio_h / ratio_w;
            let diagonal = (width * width + height * height).sqrt();
            (width, height, diagonal)
        }
        MeasurementKind::Height => {
            let height = measured_in;
            let width = height * ratio_w / ratio_h;
            let diagonal = (width * width + height * height).sqrt();
            (width, height, diagonal)
        }
    };

    if !width_in.is_finite() || !height_in.is_finite() || !diagonal_in.is_finite() {
        bail!("Calculated non-finite dimensions; check inputs");
    }

    Ok(ScreenSize {
        diagonal_in,
        width_in,
        height_in,
    })
}

fn to_inches(value: f64, unit: Unit) -> f64 {
    match unit {
        Unit::Inches => value,
        Unit::Centimeters => value / CM_PER_INCH,
    }
}

fn to_centimeters(inches: f64) -> f64 {
    inches * CM_PER_INCH
}

fn format_dual_unit(inches: f64) -> String {
    format!(
        "{}in or {}cm",
        format_number(inches),
        format_number(to_centimeters(inches))
    )
}

fn format_number(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if (rounded - rounded.round()).abs() < 1e-9 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inches_diagonal_input() {
        let parsed = parse_measurement("7id").expect("should parse");
        assert_eq!(parsed.0, 7.0);
        assert_eq!(parsed.1, Unit::Inches);
        assert_eq!(parsed.2, MeasurementKind::Diagonal);
    }

    #[test]
    fn parses_centimeters_height_input() {
        let parsed = parse_measurement("20cmh").expect("should parse");
        assert_eq!(parsed.0, 20.0);
        assert_eq!(parsed.1, Unit::Centimeters);
        assert_eq!(parsed.2, MeasurementKind::Height);
    }

    #[test]
    fn computes_16_9_from_diagonal_inches() {
        let size = calculate_screen_size(7.0, MeasurementKind::Diagonal, 16.0, 9.0)
            .expect("calculation should succeed");

        assert!((size.width_in - 6.1).abs() < 0.05);
        assert!((size.height_in - 3.4).abs() < 0.05);
        assert!((size.diagonal_in - 7.0).abs() < 0.0001);
    }

    #[test]
    fn computes_diagonal_from_width() {
        let size = calculate_screen_size(20.0, MeasurementKind::Width, 16.0, 10.0)
            .expect("calculation should succeed");

        assert!((size.height_in - 12.5).abs() < 0.0001);
        assert!((size.diagonal_in - 23.585).abs() < 0.01);
    }

    #[test]
    fn parses_inputs_in_original_order() {
        let (_, ratio, ratio_text) = parse_inputs("7id", "16:9").expect("should parse");
        assert_eq!(ratio, (16.0, 9.0));
        assert_eq!(ratio_text, "16:9");
    }

    #[test]
    fn parses_inputs_in_swapped_order() {
        let (_, ratio, ratio_text) = parse_inputs("16:9", "7id").expect("should parse");
        assert_eq!(ratio, (16.0, 9.0));
        assert_eq!(ratio_text, "16:9");
    }
}
