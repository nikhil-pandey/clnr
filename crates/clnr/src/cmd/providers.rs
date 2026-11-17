use clnr_core::{Detection, human};

use comfy_table::{Cell, Color};

use crate::app::App;
use crate::style::HEADER;
use crate::table::{self, Column};

const COLUMNS: &[Column] = &[
    Column::new("WHAT"),
    Column::new("PATH"),
    Column::new("FROM").min_width(80),
    Column::new("EXISTS"),
];

pub fn run(app: &App) {
    let env = &app.env;
    for (i, provider) in app.registry.providers().iter().enumerate() {
        if i > 0 {
            anstream::println!();
        }
        let status = match provider.detect(env) {
            Detection::Installed(path) => format!("installed ({})", human::path(&path, env.home())),
            Detection::DataOnly => "not on PATH, data found".to_owned(),
            Detection::Missing => "not found".to_owned(),
        };
        anstream::println!(
            "{HEADER}{}{HEADER:#} ({}): {status}",
            provider.id(),
            provider.name()
        );

        let rows = provider
            .globals(env)
            .into_iter()
            .map(|g| {
                let found = g.path.exists();
                vec![
                    Cell::new(&g.label),
                    Cell::new(human::path(&g.path, env.home())),
                    Cell::new(g.source.to_string()),
                    table::cell(
                        if found { "found" } else { "-" },
                        found.then_some(Color::Green),
                    ),
                ]
            })
            .collect();
        table::print(COLUMNS, rows);

        let rules: Vec<String> = provider
            .project_rules()
            .iter()
            .map(|r| format!("{}/", r.dir))
            .collect();
        if !rules.is_empty() {
            anstream::println!("  project dirs: {}", rules.join(", "));
        }
    }
    let roots: Vec<String> = app
        .options
        .roots
        .iter()
        .map(|r| human::path(r, env.home()))
        .collect();
    anstream::println!("\nroots: {}", roots.join(", "));
}
