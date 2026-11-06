// SPDX-License-Identifier: MIT
//! The suppliers' shops by region (mitcad#950; `docs/libraries.md`,
//! "Shops and product links"): MitCad's own list of the suppliers the
//! libraries' rows name (`suppliers[].id`), each with how its canonical
//! part number is written in its shops' addresses and its shops by region
//! (an ISO 3166-1 alpha-2 code: `FI`), each with a product address pattern.
//! A library stores part numbers only (mitcad#949); the product links are
//! made here when they are shown, for the region the user chose in
//! Preferences.
//!
//! The list lives in MitCad and not in a library: a design keeps the
//! library version it was inserted from, so patterns stored with a
//! library's version would never follow a shop that moves; and the parts
//! list and the browser show the articles a design keeps, also without its
//! library. A library's supplier id names MitCad's supplier of that id; a
//! supplier MitCad does not list gets no links.
//!
//! The shops' names and patterns are MitCad's own data (MIT), written
//! from the shops' public addresses; nothing is copied from a catalogue.

use std::fmt::Write as _;

use serde::Serialize;
use serde_json::{Value, json};

use crate::library::SupplierPart;

/// How a supplier's canonical part number (its groups separated by single
/// spaces, as the libraries write it: `0084 6 8`) is written in its
/// shops' addresses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartNumberForm {
    /// The width each group but the last is filled to with spaces on the
    /// right (`[4, 3]`: `0084 6 8` is `0084` `6  ` `8`); a group without a
    /// width, and the last, as it is.
    pub group_widths: &'static [usize],
    /// The text between the groups (`""`: none, `" "`: as written).
    pub separator: &'static str,
}

impl PartNumberForm {
    /// The canonical form as it is: groups separated by single spaces.
    pub const AS_WRITTEN: PartNumberForm = PartNumberForm {
        group_widths: &[],
        separator: " ",
    };

    /// The part number in the shops' form (`0084 6 8` -> `00846  8`).
    pub fn shop_form(&self, canonical: &str) -> String {
        let groups: Vec<&str> = canonical.split_whitespace().collect();
        let last = groups.len().saturating_sub(1);
        let filled: Vec<String> = groups
            .iter()
            .enumerate()
            .map(|(i, group)| match self.group_widths.get(i) {
                Some(&width) if i < last => format!("{group:<width$}"),
                _ => (*group).to_owned(),
            })
            .collect();
        filled.join(self.separator)
    }
}

/// A supplier's shop in one region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shop {
    /// The region it sells in: an ISO 3166-1 alpha-2 code (`FI`).
    pub region: &'static str,
    /// Its name for people (`Würth eShop`).
    pub name: &'static str,
    /// Its own address.
    pub url: &'static str,
    /// The address of a product: `{part_number}` is the part number in the
    /// supplier's [`PartNumberForm`], percent-encoded.
    pub product_url: &'static str,
}

impl Shop {
    /// The address of the product of a canonical part number in this shop.
    pub fn product_link(&self, form: &PartNumberForm, part_number: &str) -> String {
        self.product_url.replace(
            "{part_number}",
            &percent_encoded(&form.shop_form(part_number)),
        )
    }
}

/// A supplier of MitCad's list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Supplier {
    /// The id the libraries' `suppliers` give it (`wurth`).
    pub id: &'static str,
    pub name: &'static str,
    /// Its own site.
    pub url: &'static str,
    /// How its part numbers are written in its shops' addresses.
    pub part_number: PartNumberForm,
    /// Its shops, at most one per region.
    pub shops: &'static [Shop],
}

impl Supplier {
    /// Its shop in `region` (compared without case), if any.
    pub fn shop(&self, region: &str) -> Option<&'static Shop> {
        let region = region.trim();
        if region.is_empty() {
            return None;
        }
        self.shops
            .iter()
            .find(|s| s.region.eq_ignore_ascii_case(region))
    }
}

/// A region the shops sell in, with its name for people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Region {
    pub id: &'static str,
    pub name: &'static str,
}

/// The names of the regions the shops below sell in.
pub const REGIONS: &[Region] = &[Region {
    id: "FI",
    name: "Finland",
}];

/// MitCad's suppliers: those the libraries' rows name. A supplier whose
/// shops' product addresses are not known has none (no links, its part
/// numbers shown as they are).
pub const SUPPLIERS: &[Supplier] = &[
    // The article number in its groups (`0084 6 8`) is the shop's article
    // key with the middle group filled to three characters when a third
    // follows (`00846  8`, docs/libraries.md, MitCad's parts library).
    Supplier {
        id: "wurth",
        name: "Würth",
        url: "https://www.wurth.fi",
        part_number: PartNumberForm {
            group_widths: &[4, 3],
            separator: "",
        },
        shops: &[Shop {
            region: "FI",
            name: "Würth eShop",
            url: "https://eshop.wurth.fi",
            product_url: "https://eshop.wurth.fi/product/{part_number}.sku/fi/FI/EUR/",
        }],
    },
    // The product code as its catalogue writes it. Its shop's product
    // addresses are not known (its site refuses scripted reads): no shop.
    Supplier {
        id: "etra",
        name: "Etra",
        url: "https://www.etra.fi",
        part_number: PartNumberForm::AS_WRITTEN,
        shops: &[],
    },
];

/// The supplier of an id (compared without case), if MitCad lists it.
pub fn supplier(id: &str) -> Option<&'static Supplier> {
    let id = id.trim();
    SUPPLIERS.iter().find(|s| s.id.eq_ignore_ascii_case(id))
}

/// The regions some shop sells in, by name.
pub fn regions() -> Vec<&'static Region> {
    let mut out: Vec<&'static Region> = REGIONS
        .iter()
        .filter(|r| SUPPLIERS.iter().any(|s| s.shop(r.id).is_some()))
        .collect();
    out.sort_by_key(|r| r.name);
    out
}

/// A region's name for people; the code itself when it is not listed.
pub fn region_name(region: &str) -> String {
    REGIONS
        .iter()
        .find(|r| r.id.eq_ignore_ascii_case(region.trim()))
        .map_or_else(|| region.trim().to_uppercase(), |r| r.name.to_owned())
}

/// Whether a text is a region code: two letters `A-Z`.
pub fn valid_region(region: &str) -> bool {
    region.len() == 2 && region.bytes().all(|b| b.is_ascii_uppercase())
}

/// The suppliers with a shop in `region`, in the list's order.
pub fn region_suppliers(region: &str) -> Vec<&'static Supplier> {
    SUPPLIERS
        .iter()
        .filter(|s| s.shop(region).is_some())
        .collect()
}

/// The region a locale's territory names (`fi_FI`, `en-FI`, `FI`) when a
/// shop sells there; else none (empty): the Preferences' default.
pub fn default_region(locale: &str) -> String {
    let name = locale.split(['.', '@']).next().unwrap_or_default().trim();
    // A language alone (`fi`) names no territory; a code alone is one.
    let territory = match name.rsplit_once(['_', '-']) {
        Some((_, territory)) => territory.to_uppercase(),
        None if valid_region(name) => name.to_owned(),
        None => String::new(),
    };
    if valid_region(&territory) && !region_suppliers(&territory).is_empty() {
        territory
    } else {
        String::new()
    }
}

/// The shops the user chose (supplier ids) that sell in `region`, each
/// once, in the list's order: the Supplier filters' default (empty: All).
pub fn chosen_shops(region: &str, shops: &[String]) -> Vec<&'static str> {
    region_suppliers(region)
        .into_iter()
        .filter(|s| shops.iter().any(|id| id.trim().eq_ignore_ascii_case(s.id)))
        .map(|s| s.id)
        .collect()
}

/// A product link: the shop's name and the address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductLink {
    pub shop: String,
    pub url: String,
}

/// The product link of a supplier's canonical part number in `region`:
/// none without a region, for a supplier MitCad does not list or one
/// without a shop there, and for an empty part number.
pub fn product_link(supplier_id: &str, part_number: &str, region: &str) -> Option<ProductLink> {
    if part_number.trim().is_empty() {
        return None;
    }
    let supplier = supplier(supplier_id)?;
    let shop = supplier.shop(region)?;
    Some(ProductLink {
        shop: shop.name.to_owned(),
        url: shop.product_link(&supplier.part_number, part_number),
    })
}

/// The product link of an article in `region`: an article of one region
/// only (`region` on the article: its supplier's part numbers differ by
/// country) has none elsewhere.
pub fn article_link(part: &SupplierPart, region: &str) -> Option<ProductLink> {
    if !part.region.trim().is_empty() && !part.region.trim().eq_ignore_ascii_case(region.trim()) {
        return None;
    }
    product_link(&part.supplier, &part.part_number, region)
}

/// Articles as JSON (`supplier_parts` of `library_show`, `parts_list`...)
/// with each one's `product_url` and `shop` in `region` where it has a
/// link; the others as they are.
pub fn with_links(parts: &[Value], region: &str) -> Vec<Value> {
    parts
        .iter()
        .map(|value| {
            let mut out = value.clone();
            let part: SupplierPart = serde_json::from_value(value.clone()).unwrap_or_default();
            if let (Some(link), Some(object)) = (article_link(&part, region), out.as_object_mut()) {
                object.insert("product_url".to_owned(), json!(link.url));
                object.insert("shop".to_owned(), json!(link.shop));
            }
            out
        })
        .collect()
}

/// The list as JSON (the library command `supplier_shops`): the regions
/// with shops, the suppliers with their shops, and the region `locale`
/// gives ([`default_region`]).
pub fn shops_json(locale: &str) -> Value {
    let regions: Vec<Value> = regions()
        .into_iter()
        .map(|r| json!({"id": r.id, "name": r.name}))
        .collect();
    json!({
        "regions": regions,
        "suppliers": SUPPLIERS,
        "default_region": default_region(locale),
    })
}

/// Percent-encodes a text for an address: every byte but `A-Z`, `a-z`,
/// `0-9`, `-`, `.`, `_` and `~` as `%XX` (a space is `%20`).
pub fn percent_encoded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            // Writing to a String does not fail.
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wurth() -> &'static Supplier {
        supplier("wurth").unwrap()
    }

    #[test]
    fn a_canonical_part_number_in_the_shops_form() {
        let form = &wurth().part_number;
        // The middle group filled to three when a third follows.
        assert_eq!(form.shop_form("0084 6 8"), "00846  8");
        assert_eq!(form.shop_form("0084 25 6"), "008425 6");
        assert_eq!(form.shop_form("0326 999 3"), "03269993");
        // Two groups: the last as it is.
        assert_eq!(form.shop_form("0317 910"), "0317910");
        assert_eq!(form.shop_form("5149 5"), "51495");
        // Spaces at the ends and between the groups do not count.
        assert_eq!(form.shop_form("  0084  6 8 "), "00846  8");
        assert_eq!(form.shop_form(""), "");
        // As written: the groups separated by single spaces.
        assert_eq!(
            PartNumberForm::AS_WRITTEN.shop_form("701 200  206"),
            "701 200 206"
        );
        let dashes = PartNumberForm {
            group_widths: &[],
            separator: "-",
        };
        assert_eq!(dashes.shop_form("A 6 10"), "A-6-10");
    }

    #[test]
    fn product_links_follow_the_region_and_the_pattern() {
        let link = product_link("wurth", "0084 6 8", "FI").unwrap();
        assert_eq!(link.shop, "Würth eShop");
        assert_eq!(
            link.url,
            "https://eshop.wurth.fi/product/00846%20%208.sku/fi/FI/EUR/"
        );
        // Ids and regions without case.
        assert_eq!(product_link("Wurth", "0084 6 8", "fi").unwrap(), link);
        // No region, no shop in the region, no shop at all, a supplier
        // MitCad does not list, no part number: no link.
        assert_eq!(product_link("wurth", "0084 6 8", ""), None);
        assert_eq!(product_link("wurth", "0084 6 8", "SE"), None);
        assert_eq!(product_link("etra", "701200206008", "FI"), None);
        assert_eq!(product_link("acme", "A 6 10", "FI"), None);
        assert_eq!(product_link("wurth", " ", "FI"), None);
    }

    #[test]
    fn an_article_of_one_region_links_only_there() {
        let mut part = SupplierPart {
            supplier: "wurth".to_owned(),
            part_number: "0084 6 8".to_owned(),
            ..SupplierPart::default()
        };
        assert!(article_link(&part, "FI").is_some());
        part.region = "SE".to_owned();
        assert_eq!(article_link(&part, "FI"), None);
        part.region = "fi".to_owned();
        assert!(article_link(&part, "FI").is_some());
    }

    #[test]
    fn articles_as_json_get_their_links() {
        let parts = vec![
            json!({"supplier": "wurth", "supplier_name": "Würth", "part_number": "0317 910",
                   "material": "8", "finish": "zinc-plated"}),
            json!({"supplier": "etra", "part_number": "701200206008"}),
            json!("not an article"),
        ];
        let linked = with_links(&parts, "FI");
        assert_eq!(
            linked[0]["product_url"],
            "https://eshop.wurth.fi/product/0317910.sku/fi/FI/EUR/"
        );
        assert_eq!(linked[0]["shop"], "Würth eShop");
        assert_eq!(linked[0]["material"], "8");
        // Without a link: as it was.
        assert_eq!(linked[1], parts[1]);
        assert_eq!(linked[2], parts[2]);
        assert_eq!(with_links(&parts, ""), parts);
    }

    #[test]
    fn the_region_and_the_shops_as_the_defaults() {
        // The locale's territory when a shop sells there.
        assert_eq!(default_region("fi_FI.UTF-8"), "FI");
        assert_eq!(default_region("en-FI"), "FI");
        assert_eq!(default_region("FI"), "FI");
        assert_eq!(default_region("en_US"), "");
        assert_eq!(default_region("fi"), "");
        assert_eq!(default_region("C"), "");
        assert_eq!(default_region(""), "");
        // The shops chosen that sell in the region, in the list's order.
        let chosen = ["etra".to_owned(), "Wurth".to_owned(), "acme".to_owned()];
        assert_eq!(chosen_shops("FI", &chosen), ["wurth"]);
        assert!(chosen_shops("", &chosen).is_empty());
        assert!(chosen_shops("FI", &[]).is_empty());
        let suppliers: Vec<&str> = region_suppliers("FI").iter().map(|s| s.id).collect();
        assert_eq!(suppliers, ["wurth"]);
        assert_eq!(region_name("fi"), "Finland");
        assert_eq!(region_name("se"), "SE");
    }

    #[test]
    fn the_list_is_well_formed() {
        for (i, supplier) in SUPPLIERS.iter().enumerate() {
            assert!(
                SUPPLIERS[..i].iter().all(|s| s.id != supplier.id),
                "{} twice",
                supplier.id
            );
            assert!(supplier.url.starts_with("https://"), "{}", supplier.id);
            for (k, shop) in supplier.shops.iter().enumerate() {
                assert!(
                    valid_region(shop.region),
                    "{}: {}",
                    supplier.id,
                    shop.region
                );
                assert!(
                    REGIONS.iter().any(|r| r.id == shop.region),
                    "{}: {} has no name",
                    supplier.id,
                    shop.region
                );
                assert!(
                    supplier.shops[..k].iter().all(|s| s.region != shop.region),
                    "{}: two shops in {}",
                    supplier.id,
                    shop.region
                );
                assert!(shop.url.starts_with("https://"));
                assert!(shop.product_url.starts_with("https://"));
                assert!(shop.product_url.contains("{part_number}"));
            }
        }
        let answer = shops_json("fi_FI");
        assert_eq!(answer["default_region"], "FI");
        assert_eq!(answer["regions"], json!([{"id": "FI", "name": "Finland"}]));
        assert_eq!(answer["suppliers"][0]["id"], "wurth");
        assert_eq!(
            answer["suppliers"][0]["part_number"]["group_widths"],
            json!([4, 3])
        );
        assert_eq!(answer["suppliers"][0]["shops"][0]["region"], "FI");
        assert_eq!(answer["suppliers"][1]["shops"], json!([]));
    }

    #[test]
    fn percent_encoding_keeps_only_the_unreserved() {
        assert_eq!(percent_encoded("00846  8"), "00846%20%208");
        assert_eq!(percent_encoded("A-6_10.x~"), "A-6_10.x~");
        assert_eq!(percent_encoded("a/b?c"), "a%2Fb%3Fc");
        assert_eq!(percent_encoded("ä"), "%C3%A4");
    }
}
