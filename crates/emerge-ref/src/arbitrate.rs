//! Pro-rata sharing with exact remainders (spec §7.2 step 3).

/// Shares `available` µ between `requests`.
///
/// If the requests fit, each is granted in full. Otherwise each gets the floor of its
/// pro-rata share, and the leftover µ go one at a time to the largest fractional parts.
///
/// Among equal fractional parts, the request with the smaller `tie(position)` goes first,
/// then the lower position. In the world, `tie` is a Philox draw (spec §7.2, §10.2).
pub fn pro_rata(requests: &[u64], available: u64, tie: impl Fn(usize) -> u32) -> Vec<u64> {
    let total: u128 = requests.iter().map(|&r| r as u128).sum();
    if total <= available as u128 {
        return requests.to_vec();
    }
    let mut grants = Vec::with_capacity(requests.len());
    let mut fractions = Vec::with_capacity(requests.len());
    for &r in requests {
        let scaled = r as u128 * available as u128;
        grants.push((scaled / total) as u64);
        fractions.push(scaled % total);
    }
    let leftover = available - grants.iter().sum::<u64>();
    let mut order: Vec<usize> = (0..requests.len()).collect();
    let draws: Vec<u32> = (0..requests.len()).map(tie).collect();
    order.sort_by(|&a, &b| fractions[b].cmp(&fractions[a]).then(draws[a].cmp(&draws[b])).then(a.cmp(&b)));
    for &i in order.iter().take(leftover as usize) {
        grants[i] += 1;
    }
    grants
}

#[cfg(test)]
mod tests {
    use super::pro_rata as pr;

    fn pro_rata(requests: &[u64], available: u64) -> Vec<u64> {
        pr(requests, available, |_| 0)
    }

    #[test]
    fn fits_in_full() {
        assert_eq!(pro_rata(&[3, 4], 10), vec![3, 4]);
    }

    #[test]
    fn exact_shares() {
        assert_eq!(pro_rata(&[600, 600, 300], 1_000), vec![400, 400, 200]);
    }

    #[test]
    fn leftover_goes_to_largest_fraction() {
        // 1000 × 500/1600 = 312.5, 1000 × 1100/1600 = 687.5: tie, equal draws, first index wins.
        assert_eq!(pro_rata(&[500, 1_100], 1_000), vec![313, 687]);
        // 2 × 1/3 each: fractions equal, the first indices get the leftover µ.
        assert_eq!(pro_rata(&[1, 1, 1], 2), vec![1, 1, 0]);
    }

    #[test]
    fn ties_follow_the_draws() {
        // Equal fractions: the smallest draw wins, whatever its position.
        assert_eq!(pr(&[1, 1, 1], 1, |i| [9, 2, 5][i]), vec![0, 1, 0]);
        // Equal draws fall back to position.
        assert_eq!(pr(&[1, 1, 1], 1, |_| 4), vec![1, 0, 0]);
    }

    #[test]
    fn never_exceeds_available() {
        let g = pro_rata(&[7, 13, 29, 1], 31);
        assert_eq!(g.iter().sum::<u64>(), 31);
    }
}
