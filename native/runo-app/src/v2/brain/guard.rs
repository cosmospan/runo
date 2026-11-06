//! Guard rails: asks that are hard or impossible to undo wait for a yes.
//!
//! Plain rules, not a model: a rule that misses is a bug to fix here, and a
//! model's guess is not something to trust with production or someone's data.
//! What's caught is what the person asked for; the engines' own permission
//! settings still govern each command a bot runs.

use super::words;

/// Phrases that mean something can't be taken back, with why, in plain words.
const RISKS: [(&[&str], &str); 11] = [
    (
        &[
            "force push",
            "force-push",
            "push --force",
            "push -f",
            "rewrite history",
            "rewrite the history",
        ],
        "it rewrites shared history, which can lose other people's work",
    ),
    (
        &[
            "rm -rf",
            "delete all",
            "delete every",
            "wipe",
            "remove all files",
            "delete the folder",
            "delete the repo",
            "delete the directory",
        ],
        "it deletes files in bulk",
    ),
    (
        &[
            "drop table",
            "drop database",
            "truncate table",
            "delete from",
            "drop the table",
            "drop the database",
            "reset the database",
            "wipe the database",
        ],
        "it destroys data in a database",
    ),
    (
        &[
            "to prod",
            "to production",
            "in prod",
            "in production",
            "on prod",
            "on production",
            "live site",
            "production database",
            "prod db",
        ],
        "it touches production",
    ),
    (
        &[
            "deploy to",
            "deploy it",
            "deploy now",
            "release it",
            "publish to npm",
            "publish the package",
            "cargo publish",
            "npm publish",
        ],
        "it ships something to the outside world",
    ),
    (
        &[
            "api key",
            "secret key",
            "private key",
            ".env",
            "credentials",
            "rotate the key",
            "rotate the secret",
        ],
        "it touches secrets",
    ),
    (
        &[
            "send an email",
            "send emails",
            "email everyone",
            "email all",
            "send a message to",
            "post to slack",
            "tweet",
        ],
        "it sends something to other people",
    ),
    (
        &[
            "charge the customer",
            "charge customers",
            "charge the card",
            "refund the",
            "refund all",
            "payment to",
            "pay the invoice",
        ],
        "it moves money",
    ),
    (
        &[
            "merge to main",
            "merge into main",
            "push to main",
            "push to master",
            "merge to master",
        ],
        "it changes the main branch directly",
    ),
    (
        &[
            "delete the branch",
            "delete branches",
            "close all issues",
            "close all prs",
            "delete issues",
            "delete the pr",
        ],
        "it deletes work on GitHub",
    ),
    (
        &[
            "chmod 777",
            "sudo ",
            "disable auth",
            "turn off auth",
            "disable the firewall",
        ],
        "it weakens security",
    ),
];

/// Asking about a risky thing isn't doing it.
fn only_asks(text: &str) -> bool {
    let ws = words(text);
    let Some(first) = ws.first() else {
        return false;
    };
    matches!(
        first.as_str(),
        "how"
            | "why"
            | "what"
            | "where"
            | "who"
            | "explain"
            | "is"
            | "are"
            | "does"
            | "review"
            | "check"
            | "audit"
            | "find"
    ) || ws
        .iter()
        .any(|w| w == "don't" || w == "never" || w == "without")
}

/// Why `text` needs a yes before a bot starts on it, or None when it doesn't.
pub fn risk(text: &str) -> Option<String> {
    if only_asks(text) {
        return None;
    }
    let lower = format!("{} ", text.to_lowercase());
    let found: Vec<&str> = RISKS
        .iter()
        .filter(|(cues, _)| cues.iter().any(|c| lower.contains(c)))
        .map(|(_, why)| *why)
        .collect();
    match found.as_slice() {
        [] => None,
        [one] => Some(format!("This needs your yes first: {one}.")),
        [first, rest @ ..] => Some(format!(
            "This needs your yes first: {first}, and {}.",
            rest.join(", and ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_to_undo_asks_wait_for_a_yes() {
        for t in [
            "force push the rebased branch",
            "drop table users and recreate it",
            "deploy the new pricing page to production",
            "rotate the stripe api key in .env",
            "email everyone on the waitlist",
            "rm -rf the build folder and the caches",
        ] {
            assert!(risk(t).is_some(), "{t}");
        }
        let two = risk("deploy to prod and email all customers").unwrap();
        assert!(two.contains("production") && two.contains("sends"), "{two}");
    }

    #[test]
    fn everyday_work_and_questions_go_ahead() {
        for t in [
            "fix the login redirect",
            "add a dark mode toggle",
            "how does our deploy script work?",
            "review the payment code for risks",
            "make sure we never log the password",
            "write tests for the discount code",
            "fix the password reset email",
            "the token refresh fails after an hour",
            "fix the deploy script",
            "the buy button is misaligned",
        ] {
            assert!(risk(t).is_none(), "{t}: {:?}", risk(t));
        }
    }
}
