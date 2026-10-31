//! `PRAGMA integrity_check`: every tree walked and every page accounted for.

use super::catalog::Catalog;
use crate::btree::be32;
use crate::error::Result;
use crate::pager::Pager;
use crate::record;
use crate::schema;
use crate::tree::{self, Findings, Tree, compare_entries};

/// Checks the whole database; an empty list means nothing is wrong.
pub fn integrity(pager: &mut Pager) -> Result<Vec<String>> {
    let pages = pager.page_count();
    let mut found = Findings::new(pages);
    let objects = schema::read(pager)?;
    let catalog = Catalog::build(&objects);

    // The freelist.
    let (mut trunk, mut seen) = (pager.header().freelist_trunk, 0_u32);
    let mut hops = 0;
    while trunk != 0 {
        hops += 1;
        if hops > pages {
            found.problems.push("the freelist loops".into());
            break;
        }
        found.claim(trunk, "the freelist");
        seen += 1;
        let page = pager.page(trunk)?;
        let count = be32(&page, 4);
        for i in 0..count as usize {
            if 8 + 4 * i + 4 > page.len() {
                found.problems.push(format!(
                    "Page {trunk}: the freelist names more pages than fit"
                ));
                break;
            }
            found.claim(be32(&page, 8 + 4 * i), "the freelist");
            seen += 1;
        }
        trunk = be32(&page, 0);
    }
    if seen != pager.header().freelist_count {
        found.problems.push(format!(
            "the freelist holds {seen} pages but the header says {}",
            pager.header().freelist_count
        ));
    }

    // The schema table itself.
    tree::check_tree(pager, 1, Tree::Table, "the schema table", &mut found);

    // Every table and index.
    let mut table_rows = std::collections::HashMap::new();
    let mut index_entries: Vec<(String, Vec<Vec<crate::Value>>)> = Vec::new();
    for o in objects.iter().filter(|o| o.root_page != 0) {
        let tree_kind = match o.kind {
            schema::ObjectKind::Table => Tree::Table,
            schema::ObjectKind::Index => Tree::Index,
            _ => continue,
        };
        let by = format!(
            "{} {}",
            if tree_kind == Tree::Table {
                "table"
            } else {
                "index"
            },
            o.name
        );
        let contents = tree::check_tree(pager, o.root_page, tree_kind, &by, &mut found);
        if tree_kind == Tree::Table {
            table_rows.insert(o.name.to_ascii_lowercase(), contents.rows);
        } else {
            // Compared with the table below, once all trees are read.
            index_entries.push((o.name.clone(), contents.entries));
        }
    }

    // An index holds exactly what its table's rows give.
    for (name, actual) in index_entries {
        let Some((table, index)) = catalog.tables.iter().find_map(|t| {
            t.indexes
                .iter()
                .find(|i| i.name.eq_ignore_ascii_case(&name))
                .map(|i| (t, i))
        }) else {
            continue;
        };
        let Some(rows) = table_rows.get(&table.name.to_ascii_lowercase()) else {
            continue;
        };
        let mut expected: Vec<Vec<crate::Value>> = Vec::with_capacity(rows.len());
        for (rowid, payload) in rows {
            let Ok(values) = record::decode(payload) else {
                continue;
            };
            let values = table.expand_row(*rowid, values);
            let mut entry: Vec<crate::Value> =
                index.columns.iter().map(|&c| values[c].clone()).collect();
            entry.push(crate::Value::Integer(*rowid));
            expected.push(entry);
        }
        expected.sort_by(|a, b| compare_entries(a, b));
        if expected.len() != actual.len() {
            found.problems.push(format!(
                "wrong # of entries in index {name}: the table has {} rows, the index {} entries",
                expected.len(),
                actual.len()
            ));
        } else if let Some(at) = expected
            .iter()
            .zip(&actual)
            .position(|(a, b)| compare_entries(a, b) != std::cmp::Ordering::Equal)
        {
            let rowid = expected[at]
                .last()
                .and_then(crate::Value::as_integer)
                .unwrap_or(0);
            found.problems.push(format!("row {rowid} is missing from index {name} (or the index has an entry the table does not)"));
        }
    }

    // Every page belongs to something.
    for (i, owner) in found.owner.clone().iter().enumerate() {
        if owner.is_none() {
            found.problems.push(format!("Page {}: never used", i + 1));
        }
    }
    found.problems.truncate(100);
    Ok(found.problems)
}
