//! Six-face connected components on the uniform voxel grid.
use super::PressureGauge;
use std::collections::VecDeque;

#[derive(Clone, Copy)]
struct Span {
    row: usize,
    first: usize,
    last: usize,
}

// Claim the entire x run now, so several neighbours cannot queue it twice.
fn claim(mask: &mut [u32], width: usize, row: usize, x: usize, tag: u32) -> Span {
    let cells = &mut mask[row..row + width];
    let mut first = x;
    let mut last = x;
    while first > 0 && cells[first - 1] == 1 {
        first -= 1;
    }
    while last + 1 < width && cells[last + 1] == 1 {
        last += 1;
    }
    cells[first..=last].fill(tag);
    Span { row, first, last }
}

pub(super) fn anchors(
    mask: &mut [u32],
    dims: [u32; 3],
    periodic: [bool; 3],
) -> (usize, PressureGauge) {
    let [width, height, depth] = dims.map(|v| v as usize);
    let plane = width * height;
    let mut queue = VecDeque::new();
    let mut gauge = PressureGauge::default();
    let mut components = 0;
    for root in 0..mask.len() {
        if mask[root] != 1 {
            continue;
        }
        components += 1;
        let tag = (root as u32) << 2 | 3;
        let x = root % width;
        queue.push_back(claim(mask, width, root - x, x, tag));
        let mut size = 0;
        while let Some(Span { row, first, last }) = queue.pop_front() {
            size += (last - first + 1) as u32;
            if periodic[0] {
                if first == 0 && mask[row + width - 1] == 1 {
                    queue.push_back(claim(mask, width, row, width - 1, tag));
                }
                if last + 1 == width && mask[row] == 1 {
                    queue.push_back(claim(mask, width, row, 0, tag));
                }
            }

            let y = row / width % height;
            let z = row / plane;
            for (position, length, stride, wraps) in [
                (y, height, width, periodic[1]),
                (z, depth, plane, periodic[2]),
            ] {
                if length == 1 {
                    continue;
                }
                let lower = if position > 0 {
                    Some(row - stride)
                } else if wraps {
                    Some(row + (length - 1) * stride)
                } else {
                    None
                };
                let upper = if position + 1 < length {
                    Some(row + stride)
                } else if wraps {
                    Some(row - (length - 1) * stride)
                } else {
                    None
                };
                for next in [lower, upper].into_iter().flatten() {
                    let mut x = first;
                    while x <= last {
                        if mask[next + x] == 1 {
                            let span = claim(mask, width, next, x, tag);
                            x = span.last + 1;
                            queue.push_back(span);
                        } else {
                            x += 1;
                        }
                    }
                }
            }
        }
        mask[root] = (root as u32) << 2 | 2;
        if size > gauge.size {
            gauge.largest_pinned_size = gauge.size;
            gauge.size = size;
            gauge.root = root as u32;
        } else {
            gauge.largest_pinned_size = gauge.largest_pinned_size.max(size);
        }
    }
    // Tag 3 distinguishes visited cells even in the component rooted at zero.
    for cell in mask {
        if *cell & 3 == 3 {
            *cell -= 2;
        }
    }
    (components, gauge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_spans_keep_the_first_voxel_as_each_component_reference() {
        let mut mask = vec![0; 7 * 4 * 3];
        let main = [0, 6, 21, 56, 11, 12, 13];
        for i in main.into_iter().chain([45, 46]) {
            mask[i] = 1;
        }
        let (components, gauge) = anchors(&mut mask, [7, 4, 3], [true; 3]);
        assert_eq!(
            (
                components,
                gauge.root,
                gauge.size,
                gauge.largest_pinned_size
            ),
            (2, 0, 7, 2)
        );
        for i in main {
            assert_eq!(mask[i], if i == 0 { 2 } else { 1 });
        }
        assert_eq!(mask[45], (45 << 2) | 2);
        assert_eq!(mask[46], (45 << 2) | 1);
        assert_eq!(mask.iter().filter(|&&v| v != 0).count(), 9);
    }
}
