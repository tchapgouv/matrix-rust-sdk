/*
 * Copyright (c) 2024 BWI GmbH
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use crate::password_evaluator::BWIPasswordStrength::{Medium, Strong, Weak};
use zxcvbn::{zxcvbn, Score};

#[derive(Debug)]
pub struct BWIPasswordEvaluator {}

pub enum BWIPasswordStrength {
    Weak,
    Medium,
    Strong,
}

impl BWIPasswordEvaluator {
    pub fn get_password_strength(password: &str) -> BWIPasswordStrength {
        let score = zxcvbn(password, &[]).score();
        match score {
            Score::Zero | Score::One => Weak,
            Score::Two | Score::Three => Medium,
            Score::Four => Strong,
            _ => panic!("new enum values in enum Score!"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_empty_password() {
        assert!(matches!(BWIPasswordEvaluator::get_password_strength(""), Weak));
    }

    #[test]
    fn test_std_password() {
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("s"), Weak));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("si"), Weak));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sic"), Weak));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sich"), Weak));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("siche"), Weak));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sicher"), Weak));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sicherh"), Medium));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sicherhe"), Medium));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sicherhei"), Medium));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sicherheit"), Medium));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sicherheits"), Strong));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sicherheitsp"), Strong));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sicherheitsph"), Strong));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sicherheitsphr"), Strong));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sicherheitsphra"), Strong));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sicherheitsphras"), Strong));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("sicherheitsphrase"), Strong));
    }

    #[test]
    fn test_complex_password() {
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("7"), Weak));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("7P"), Weak));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("7Pä"), Weak));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("7Pär"), Weak));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("7PärF"), Weak));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("7PärF/"), Weak));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("7PärF/@c"), Medium));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("7PärF/@ca"), Medium));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("7PärF/@ca$"), Medium));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("7PärF/@ca$1"), Strong));
        assert!(matches!(BWIPasswordEvaluator::get_password_strength("7PärF/@ca$12"), Strong));
    }
}
