//! Length-limited Huffman code lengths, shared by the deflate and zstd encoders.

/// A node of the package-merge construction: a symbol, or two nodes of the level below.
#[derive(Clone, Copy)]
enum Node {
    Leaf(usize),
    Package(usize, usize),
}

/// Optimal code lengths no longer than `max_bits` for the symbols with a non-zero frequency (the
/// package-merge algorithm). Unused symbols get length 0; a lone symbol gets length 1.
///
/// `max_bits` must be large enough for the number of used symbols (`2^max_bits >= used`).
pub fn code_lengths(frequencies: &[u32], max_bits: u8) -> Vec<u8> {
    let mut lengths = vec![0u8; frequencies.len()];
    let mut used: Vec<usize> = (0..frequencies.len())
        .filter(|&s| frequencies[s] > 0)
        .collect();
    used.sort_by_key(|&s| (frequencies[s], s));
    match used.len() {
        0 => return lengths,
        1 => {
            lengths[used[0]] = 1;
            return lengths;
        }
        _ => {}
    }
    debug_assert!(used.len() <= 1usize << max_bits);

    let leaf_count = used.len();
    let mut nodes: Vec<(u64, Node)> = used
        .iter()
        .map(|&s| (u64::from(frequencies[s]), Node::Leaf(s)))
        .collect();
    let leaves: Vec<usize> = (0..leaf_count).collect();

    let mut level = leaves.clone();
    for _ in 1..max_bits {
        let mut packages = Vec::with_capacity(level.len() / 2);
        for pair in level.as_chunks::<2>().0 {
            let weight = nodes[pair[0]].0 + nodes[pair[1]].0;
            nodes.push((weight, Node::Package(pair[0], pair[1])));
            packages.push(nodes.len() - 1);
        }
        // Merge the leaves with the packages by weight, leaves first on a tie.
        let mut merged = Vec::with_capacity(leaves.len() + packages.len());
        let (mut l, mut p) = (0, 0);
        while l < leaves.len() || p < packages.len() {
            if p == packages.len()
                || (l < leaves.len() && nodes[leaves[l]].0 <= nodes[packages[p]].0)
            {
                merged.push(leaves[l]);
                l += 1;
            } else {
                merged.push(packages[p]);
                p += 1;
            }
        }
        level = merged;
    }

    // The cheapest 2n - 2 items of the last level; a symbol's length is how many of them contain it.
    let mut stack: Vec<usize> = level.iter().take(2 * leaf_count - 2).copied().collect();
    while let Some(index) = stack.pop() {
        match nodes[index].1 {
            Node::Leaf(symbol) => lengths[symbol] += 1,
            Node::Package(a, b) => {
                stack.push(a);
                stack.push(b);
            }
        }
    }
    lengths
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kraft(lengths: &[u8]) -> f64 {
        lengths
            .iter()
            .filter(|&&l| l > 0)
            .map(|&l| 0.5f64.powi(i32::from(l)))
            .sum()
    }

    #[quench_test::test]
    fn small_alphabets() {
        assert_eq!(code_lengths(&[0, 0, 0], 15), vec![0, 0, 0]);
        assert_eq!(code_lengths(&[0, 7, 0], 15), vec![0, 1, 0]);
        assert_eq!(code_lengths(&[1, 1], 15), vec![1, 1]);
        assert_eq!(code_lengths(&[1, 1, 2], 15), vec![2, 2, 1]);
    }

    #[quench_test::test]
    fn lengths_are_complete_and_limited() {
        // Fibonacci-like frequencies make an unrestricted tree very deep.
        let mut frequencies = vec![1u32, 1];
        for i in 2..30 {
            let next = frequencies[i - 1] + frequencies[i - 2];
            frequencies.push(next);
        }
        for limit in [5u8, 8, 15] {
            let lengths = code_lengths(&frequencies, limit);
            assert!(lengths.iter().all(|&l| l <= limit && l > 0), "{lengths:?}");
            assert!((kraft(&lengths) - 1.0).abs() < 1e-12, "limit {limit}");
        }
        // Unlimited it is the textbook tree, so the rarest symbols are the deepest.
        let deep = code_lengths(&frequencies, 29);
        assert_eq!(deep[0], deep[1]);
        assert!(deep[0] > deep[29]);
    }
}
