/// Starting ELO rating for new players
pub const DEFAULT_RATING: i32 = 1200;

/// K-factor for ELO calculation
const K: f64 = 32.0;

/// Calculate new ELO rating after a match.
/// Returns the new rating for `player_rating`.
pub fn calculate_elo(player_rating: i32, opponent_rating: i32, won: bool) -> i32 {
    let expected = 1.0 / (1.0 + 10f64.powf((opponent_rating - player_rating) as f64 / 400.0));
    let score = if won { 1.0 } else { 0.0 };
    let new_rating = player_rating as f64 + K * (score - expected);
    new_rating.round() as i32
}

/// Calculate ELO change (delta) for the player
pub fn elo_change(player_rating: i32, opponent_rating: i32, won: bool) -> i32 {
    calculate_elo(player_rating, opponent_rating, won) - player_rating
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_ratings_win() {
        let new = calculate_elo(1200, 1200, true);
        assert_eq!(new, 1216);
    }

    #[test]
    fn equal_ratings_loss() {
        let new = calculate_elo(1200, 1200, false);
        assert_eq!(new, 1184);
    }

    #[test]
    fn underdog_wins() {
        let new = calculate_elo(1000, 1400, true);
        // Underdog gains more
        assert!(new - 1000 > 16);
    }

    #[test]
    fn favorite_wins() {
        let new = calculate_elo(1400, 1000, true);
        // Favorite gains less
        assert!(new - 1400 < 16);
    }

    #[test]
    fn elo_change_symmetric() {
        let gain = elo_change(1200, 1200, true);
        let loss = elo_change(1200, 1200, false);
        assert_eq!(gain, -loss);
    }
}
