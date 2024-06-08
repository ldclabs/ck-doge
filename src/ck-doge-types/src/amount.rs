pub const DOGE: u64 = 100_000_000;
pub const CENT: u64 = 1_000_000;
// https://github.com/dogecoin/dogecoin/blob/master/doc/fee-recommendation.md
// 0.01 DOGE per kilobyte transaction fee
// 0.01 DOGE dust limit (discard threshold)
// 0.001 DOGE replace-by-fee increments
pub const FEE: u64 = 1_000_000;
pub const DUST_LIMIT: u64 = 1_000_000;

pub fn fee_by_size(size: usize) -> u64 {
    (((size as u64 * FEE) as f64 / 1024f64).ceil() as u64).max(FEE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_by_size() {
        assert_eq!(fee_by_size(0), FEE);
        assert_eq!(fee_by_size(10), FEE);
        assert_eq!(fee_by_size(1000), FEE);
        assert_eq!(fee_by_size(1034), 1009766);
        assert_eq!(fee_by_size(2048), FEE * 2);
    }
}
