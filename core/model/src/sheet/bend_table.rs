// SPDX-License-Identifier: MIT
//! Manufacturing rows from the evaluated Flat Pattern (mitcad#2241).

use std::fmt::Write;

use serde::{Deserialize, Serialize};

use super::BendId;
use super::flat::FlatBend;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BendTableFormat {
    #[default]
    Csv,
    Json,
}

#[derive(Debug, Serialize)]
pub struct BendTable {
    pub bends: usize,
    pub rows: Vec<BendRow>,
}

#[derive(Debug, Serialize)]
pub struct BendRow {
    pub body: String,
    pub rule: String,
    pub thickness_mm: f64,
    pub k_factor: f64,
    pub bend: BendId,
    pub angle_deg: f64,
    pub direction: &'static str,
    pub radius_mm: f64,
    pub allowance_mm: f64,
    pub length_mm: f64,
    pub start_x_mm: f64,
    pub start_y_mm: f64,
    pub end_x_mm: f64,
    pub end_y_mm: f64,
    pub included_angle_deg: f64,
}

impl BendTable {
    pub fn from_bends(body: &str, rule: &str, thickness_mm: f64, bends: &[FlatBend]) -> Self {
        let mut table = Self {
            bends: 0,
            rows: Vec::new(),
        };
        for bend in bends.iter().filter(|bend| !bend.centerlines.is_empty()) {
            table.bends += 1;
            let angle_deg = bend.angle.to_degrees();
            for line in &bend.centerlines {
                let [start_x_mm, start_y_mm] = line.start;
                let [end_x_mm, end_y_mm] = line.end;
                table.rows.push(BendRow {
                    body: body.to_owned(),
                    rule: rule.to_owned(),
                    thickness_mm,
                    k_factor: bend.k_factor,
                    bend: bend.bend,
                    angle_deg,
                    direction: bend.direction,
                    radius_mm: bend.radius,
                    allowance_mm: bend.allowance,
                    length_mm: (end_x_mm - start_x_mm).hypot(end_y_mm - start_y_mm),
                    start_x_mm,
                    start_y_mm,
                    end_x_mm,
                    end_y_mm,
                    included_angle_deg: (180.0 - angle_deg.abs()).abs(),
                });
            }
        }
        table
    }

    pub fn csv(&self) -> String {
        let mut csv = String::from(concat!(
            "body,rule,thickness_mm,k_factor,bend,angle_deg,direction,radius_mm,",
            "allowance_mm,length_mm,start_x_mm,start_y_mm,end_x_mm,end_y_mm,included_angle_deg\r\n"
        ));
        for row in &self.rows {
            let bend = format!("{}:{}", row.bend.feature, row.bend.slot);
            write!(
                csv,
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\r\n",
                csv_text(&row.body),
                csv_text(&row.rule),
                row.thickness_mm,
                row.k_factor,
                csv_text(&bend),
                row.angle_deg,
                csv_text(row.direction),
                row.radius_mm,
                row.allowance_mm,
                row.length_mm,
                row.start_x_mm,
                row.start_y_mm,
                row.end_x_mm,
                row.end_y_mm,
                row.included_angle_deg,
            )
            .expect("writing to a String");
        }
        csv
    }
}

fn csv_text(text: &str) -> String {
    // CSV quoting escapes separators; the apostrophe keeps a spreadsheet
    // from interpreting a text cell as a formula after unquoting it.
    let formula = text.starts_with(['\t', '\r', '\n'])
        || text
            .trim_start()
            .starts_with(['=', '+', '-', '@', '＝', '＋', '－', '＠']);
    let prefix = if formula { "'" } else { "" };
    format!("\"{prefix}{}\"", text.replace('"', "\"\""))
}
