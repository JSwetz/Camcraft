
/// Hand check of #519 on a real GPU: the traced stock replaced the whole
/// stock range, teal included, so the preview never showed once
/// toolpaths existed. The incoming part now sorts last in the stock
/// range - its own range, which `render` draws after the traced stock.
#[test]
fn the_incoming_part_sorts_after_the_stock_the_traced_view_replaces() {
    use crate::viewport::{draw_order, is_overlay, GpuPart};
    let part = |stock: bool, fill: bool, cls: f32| GpuPart {
        verts: vec![[0.0, 0.0, 0.0]; 3],
        selected: false,
        stock,
        fill,
        classes: if cls > 0.0 { vec![cls; 3] } else { Vec::new() },
    };
    let incoming = part(true, false, crate::incoming_view::INCOMING_CLASS);
    let parts = vec![incoming, part(false, false, 0.0), part(true, false, 0.0), part(false, true, 0.0), part(true, false, 1.0)];
    let (order, from) = draw_order(&parts);
    assert_eq!(order.len(), 5);
    assert!(!order[0].stock, "models first");
    assert!(order[1].stock && order[2].stock && !is_overlay(order[1]) && !is_overlay(order[2]), "plain and judged stock next");
    assert_eq!(from, 3);
    assert!(is_overlay(order[3]), "the incoming stock last in the stock range");
    assert!(order[4].fill, "fills after");
    // A judged rest/gouge class is never mistaken for the overlay.
    assert!(!is_overlay(&part(true, false, 2.0)) && !is_overlay(&part(true, false, 3.0)));
}
