//! The module pages, one file per module, and what the rack needs to know about each.

mod generic;

mod decomp;
mod deexp;
mod harm;
mod hcomp;

mod bands;
mod clear;
mod dither;
mod deq;
mod dual;
mod eq6;
mod eqgraph;
mod limiter;
mod modes;
mod peq;
mod shaper;
mod smart;
mod sone;
mod sup;
mod wall;

use crate::ui::page::Page;
use crate::LAYOUT;

pub use generic::Generic;

/// The page for a chain slot, or for the dither (`None`).
pub fn page_for(slot: Option<usize>) -> Box<dyn Page> {
    let Some(info) = slot.and_then(|s| LAYOUT.slots.get(s)) else { return dither::page(LAYOUT.dither_count) };
    let n = info.count;
    match info.key {
        "" => limiter::page(n),
        "clear" => clear::page(n),
        "modes" => modes::page(n),
        "wall" => wall::page(n),
        "bands" => bands::page(n),
        "dual" => dual::page(n),
        "sone" => sone::page(n),
        "eq" => eq6::page(n),
        "shaper" => shaper::page(n),
        "peq" => peq::page(n),
        "deq" => deq::page(n),
        "sup" => sup::page(n),
        "hcomp" => hcomp::page(n),
        "decomp" => decomp::page(n),
        "deexp" => deexp::page(n),
        "harm" => harm::page(n),
        "smart" => smart::page(n),
        _ => Box::new(Generic::new(n, &[])),
    }
}

/// `(listens to the sidechain key, output can pass its ceiling)` for a slot's rack chip.
pub fn chip_flags(slot: usize, values: &[f32]) -> (bool, bool) {
    let Some(info) = LAYOUT.slots.get(slot) else { return (false, false) };
    let own = values.get(info.base..info.base + info.count).unwrap_or(&[]);
    match info.key {
        "" => limiter::chip_flags(own),
        "clear" => clear::chip_flags(own),
        "modes" => modes::chip_flags(own),
        "wall" => wall::chip_flags(own),
        "bands" => bands::chip_flags(own),
        "dual" => dual::chip_flags(own),
        "sone" => sone::chip_flags(own),
        "eq" => eq6::chip_flags(own),
        "shaper" => shaper::chip_flags(own),
        "peq" => peq::chip_flags(own),
        "deq" => deq::chip_flags(own),
        "sup" => sup::chip_flags(own),
        "hcomp" => hcomp::chip_flags(own),
        "decomp" => decomp::chip_flags(own),
        "deexp" => deexp::chip_flags(own),
        "harm" => harm::chip_flags(own),
        "smart" => smart::chip_flags(own),
        _ => (false, false),
    }
}
