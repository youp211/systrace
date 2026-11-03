//! Panel placement shared by every front end.

use crate::display::Panel;

/// Rows a panel needs in a text layout: one per widget plus its border. Also the
/// weight `columns` balances, so the TUI and GUI arrange panels the same way.
pub fn rows(panel: &Panel) -> usize {
    panel.widgets.len() + 2
}

/// Splits panels into `count` columns of indices into `panels`.
/// Highest priority goes first, each into the currently shortest column,
/// so the busiest panels sit top-left and column heights stay balanced.
pub fn columns(
    panels: &[Panel],
    count: usize,
    height: impl Fn(&Panel) -> usize,
) -> Vec<Vec<usize>> {
    let count = count.clamp(1, panels.len().max(1));
    let mut order: Vec<usize> = (0..panels.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(panels[i].priority));

    let mut cols: Vec<(usize, Vec<usize>)> = vec![(0, Vec::new()); count];
    for i in order {
        // min_by_key returns the first minimum, so ties go to the leftmost column.
        if let Some((total, col)) = cols.iter_mut().min_by_key(|(total, _)| *total) {
            *total += height(&panels[i]);
            col.push(i);
        }
    }
    cols.into_iter().map(|(_, col)| col).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(id: &'static str, priority: u8) -> Panel {
        Panel::new(id, id).priority(priority)
    }

    #[test]
    fn balances_by_priority_then_height() {
        let panels = [
            panel("a", 10),
            panel("b", 90),
            panel("c", 50),
            panel("d", 70),
        ];
        let heights = |p: &Panel| match p.id {
            "b" => 10,
            _ => 3,
        };
        // b (tall) fills column 0; d, c, a stack in column 1 until it catches up.
        assert_eq!(columns(&panels, 2, heights), [vec![1], vec![3, 2, 0]]);
        assert_eq!(columns(&panels, 1, heights), [vec![1, 3, 2, 0]]);
        // More columns than panels collapses to one per panel.
        assert_eq!(columns(&panels[..1], 4, heights), [vec![0]]);
        assert_eq!(columns(&[], 3, heights), [Vec::<usize>::new()]);
    }
}
