use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;
use optimizer_core::{OptimizationRequest, Optimizer};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "optimizer")]
#[command(about = "Cutting Stock Optimizer - Automatically calculate panel requirements", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Optimize cutting layout
    Optimize {
        /// Input file (YAML or JSON)
        #[arg(short, long)]
        input: PathBuf,

        /// Output file for result (JSON)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Generate SVG visualization from result
    Generate {
        /// Input result file (JSON)
        #[arg(short, long)]
        input: PathBuf,

        /// Output SVG file
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Optimize { input, output } => {
            optimize_command(input, output)?;
        }
        Commands::Generate { input, output } => {
            generate_command(input, output)?;
        }
    }

    Ok(())
}

fn optimize_command(input: PathBuf, output: Option<PathBuf>) -> Result<()> {
    println!("{}", "🔍 Loading input...".bright_blue());

    // Read input file
    let content = std::fs::read_to_string(&input)?;
    let request: OptimizationRequest = if input.extension().and_then(|s| s.to_str()) == Some("yaml")
        || input.extension().and_then(|s| s.to_str()) == Some("yml")
    {
        serde_yaml::from_str(&content)?
    } else {
        serde_json::from_str(&content)?
    };

    println!(
        "  {} items to cut",
        request.items.len().to_string().bright_white().bold()
    );
    println!(
        "  {} panel types available",
        request.panel_types.len().to_string().bright_white().bold()
    );
    println!();

    println!("{}", "🚀 Running optimization...".bright_blue());

    // Run optimization
    let optimizer = Optimizer::new(request)?;
    let result = optimizer.optimize()?;

    println!();
    println!("{}", "✅ Optimization complete!".bright_green().bold());
    println!();

    // Display results
    println!("{}", "📊 Results:".bright_yellow().bold());
    println!("  Panels required:");
    for (panel_id, count) in &result.panels_required {
        println!("    • {}: {} panels", panel_id.bright_white(), count);
    }
    println!();
    println!(
        "  Total panels: {}",
        result
            .summary
            .total_panels
            .to_string()
            .bright_white()
            .bold()
    );
    println!(
        "  Total waste: {:.1}%",
        result.summary.waste_percentage.to_string().bright_white()
    );

    // Display reusable remnants if available
    if let Some(reusable_area) = result.summary.reusable_remnant_area {
        println!(
            "  Reusable remnants: {:.0} sq units",
            reusable_area.to_string().bright_white()
        );
    }

    if let Some(actual_waste_pct) = result.summary.actual_waste_percentage {
        println!(
            "  Actual waste (excluding reusable): {:.1}%",
            actual_waste_pct.to_string().bright_green()
        );
    }

    // Display optional items used
    if !result.optional_items_used.is_empty() {
        println!();
        println!("  Optional items added:");
        for item in &result.optional_items_used {
            println!("    • {}", item.bright_cyan());
        }
    }

    println!();

    // Save output
    if let Some(output_path) = output {
        let json = serde_json::to_string_pretty(&result)?;
        std::fs::write(&output_path, json)?;
        println!(
            "💾 Saved result to {}",
            output_path.display().to_string().bright_white()
        );
    } else {
        // Print to stdout
        let json = serde_json::to_string_pretty(&result)?;
        println!("{}", json);
    }

    Ok(())
}

fn generate_command(input: PathBuf, output: PathBuf) -> Result<()> {
    println!("{}", "🔍 Loading result...".bright_blue());

    // Read result file
    let content = std::fs::read_to_string(&input)?;
    let result: optimizer_core::OptimizationResult = serde_json::from_str(&content)?;

    println!("{}", "🎨 Generating SVG...".bright_blue());

    // Generate SVG (simple version for now)
    let svg = generate_simple_svg(&result)?;

    // Save SVG
    std::fs::write(&output, svg)?;

    println!();
    println!(
        "{} Saved SVG to {}",
        "✅".bright_green(),
        output.display().to_string().bright_white()
    );

    Ok(())
}

fn generate_simple_svg(result: &optimizer_core::OptimizationResult) -> Result<String> {
    use std::fmt::Write;

    let mut svg = String::new();
    let margin = 20.0;
    let scale = 2.0;
    let panel_spacing = 40.0;

    let max_width = result.layouts.iter().map(|l| l.width).fold(0.0, f64::max);
    let total_height: f64 = result
        .layouts
        .iter()
        .map(|l| l.height + panel_spacing)
        .sum();

    let svg_width = (max_width / scale) + (2.0 * margin);
    let svg_height = (total_height / scale) + (2.0 * margin);

    writeln!(&mut svg, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
    writeln!(
        &mut svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
        svg_width, svg_height, svg_width, svg_height
    )?;
    writeln!(
        &mut svg,
        r##"  <rect width="100%" height="100%" fill="#f5f5f5"/>"##
    )?;

    let mut y_offset = margin;

    for layout in &result.layouts {
        let x = margin;
        let panel_width = layout.width / scale;
        let panel_height = layout.height / scale;

        writeln!(
            &mut svg,
            r##"  <rect x="{}" y="{}" width="{}" height="{}" fill="#fff" stroke="#333" stroke-width="2"/>"##,
            x, y_offset, panel_width, panel_height
        )?;

        writeln!(
            &mut svg,
            r##"  <text x="{}" y="{}" font-family="Arial" font-size="14" fill="#333">{} #{}</text>"##,
            x,
            y_offset - 5.0,
            layout.panel_type_id,
            layout.panel_number
        )?;

        for placement in &layout.placements {
            let px = x + (placement.x / scale);
            let py = y_offset + (placement.y / scale);
            let pw = placement.width / scale;
            let ph = placement.height / scale;

            writeln!(
                &mut svg,
                r##"  <rect x="{}" y="{}" width="{}" height="{}" fill="#4CAF50" stroke="#2E7D32" stroke-width="1" opacity="0.7"/>"##,
                px, py, pw, ph
            )?;

            let label = if placement.rotated {
                format!("{} (R)", placement.item_id)
            } else {
                placement.item_id.clone()
            };

            writeln!(
                &mut svg,
                r##"  <text x="{}" y="{}" font-family="Arial" font-size="10" fill="#fff" text-anchor="middle">{}</text>"##,
                px + pw / 2.0,
                py + ph / 2.0 + 3.0,
                label
            )?;
        }

        y_offset += panel_height + panel_spacing;
    }

    writeln!(&mut svg, "</svg>")?;

    Ok(svg)
}
