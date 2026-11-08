//! GitHub pull requests, issues and Actions runs for a project, through the `gh` CLI.

use serde_json::Value;

/// A review an agent wrote, waiting for the person to post it.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct ReviewDraft {
    /// COMMENT, APPROVE or REQUEST_CHANGES.
    pub event: String,
    pub body: String,
    pub comments: Vec<DraftComment>,
}

/// A comment on one line of the new version of a file.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct DraftComment {
    pub path: String,
    pub line: u64,
    pub body: String,
}

impl ReviewDraft {}

/// A session branch's pull request: state, checks and review.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct PrStatus {
    pub number: u64,
    pub url: String,
    pub title: String,
    /// OPEN, MERGED or CLOSED.
    pub state: String,
    pub draft: bool,
    /// APPROVED, CHANGES_REQUESTED, REVIEW_REQUIRED or empty.
    pub review: String,
    pub passed: usize,
    pub pending: usize,
    /// Names of the checks that failed.
    pub failed: Vec<String>,
    /// The Actions run of the first failed check, for `gh run view --log-failed`.
    pub failed_run: Option<u64>,
    /// Reviews with something to say, plus conversation comments.
    pub comments: usize,
    /// A review the session's agent wrote and nobody posted yet.
    pub draft_review: Option<ReviewDraft>,
    /// GitHub's `mergeStateStatus`: CLEAN, BLOCKED (required checks or reviews not met),
    /// UNSTABLE, BEHIND, DIRTY, and so on.
    pub merge_state: String,
    /// Auto-merge is turned on: GitHub merges it once its checks pass.
    pub auto_merge: bool,
    /// The branch the pull request merges from, and its latest commit.
    pub head_branch: String,
    pub head: String,
    /// The branch it merges into.
    pub base: String,
    /// Who opened it: the account the session's agent pushes with.
    pub author: String,
    /// MERGEABLE, CONFLICTING or UNKNOWN.
    pub mergeable: String,
    /// Each review and conversation comment with something to say, for telling new
    /// feedback from others apart from the agent's own.
    pub voices: Vec<Voice>,
}

/// One review or conversation comment on a pull request.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Voice {
    pub author: String,
    /// When it was posted, as GitHub's ISO time, which sorts as text.
    pub at: String,
    /// A review rather than a conversation comment.
    pub review: bool,
    /// Someone the person works with (see `trusted_author`): only they wake a thread.
    pub trusted: bool,
}

/// Whether GitHub's author association says the commenter is the repository's owner, a
/// member of its organization or a collaborator on it.
pub fn trusted_association(association: &str) -> bool {
    matches!(association, "OWNER" | "MEMBER" | "COLLABORATOR")
}

/// Whether a comment by `login` (with GitHub's `association`) may steer an agent: from the
/// repository's owner, members or collaborators, the pull request's own author (the
/// account the person's agent pushes with), or an app installed on the repository (a
/// `[bot]` account can only comment where it was installed). Anyone else on a public
/// repository can comment too, and their words must not become an agent's instructions.
pub fn trusted_author(login: &str, association: &str, pr_author: &str) -> bool {
    trusted_association(association)
        || (!login.is_empty() && login == pr_author)
        || login.ends_with("[bot]")
}

impl PrStatus {}

/// The fields `parse_pr_status` reads, for `gh pr view --json`.
pub const PR_FIELDS: &str = "number,url,title,state,isDraft,reviewDecision,statusCheckRollup,reviews,comments,mergeStateStatus,autoMergeRequest,headRefName,headRefOid,baseRefName,author,mergeable";

/// Parse `gh pr view --json` with `PR_FIELDS`.
pub fn parse_pr_status(json: &str) -> Option<PrStatus> {
    let v: Value = serde_json::from_str(json).ok()?;
    let number = v["number"].as_u64()?;
    let mut status = PrStatus {
        number,
        url: s(&v["url"]),
        title: s(&v["title"]),
        state: s(&v["state"]),
        draft: v["isDraft"].as_bool() == Some(true),
        review: s(&v["reviewDecision"]),
        merge_state: s(&v["mergeStateStatus"]),
        auto_merge: v["autoMergeRequest"].is_object(),
        head_branch: s(&v["headRefName"]),
        head: s(&v["headRefOid"]),
        base: s(&v["baseRefName"]),
        author: s(&v["author"]["login"]),
        mergeable: s(&v["mergeable"]),
        ..Default::default()
    };
    for check in v["statusCheckRollup"].as_array().into_iter().flatten() {
        // Check runs have status + conclusion; commit statuses have a state.
        let outcome = check["conclusion"]
            .as_str()
            .filter(|c| !c.is_empty())
            .or(check["state"].as_str())
            .unwrap_or("")
            .to_ascii_uppercase();
        let done = check["status"].as_str().is_none_or(|st| st == "COMPLETED");
        let name = check["name"]
            .as_str()
            .or(check["context"].as_str())
            .unwrap_or("check")
            .to_string();
        match outcome.as_str() {
            "SUCCESS" | "NEUTRAL" | "SKIPPED" if done => status.passed += 1,
            "FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED"
            | "STARTUP_FAILURE"
                if done =>
            {
                if status.failed_run.is_none() {
                    status.failed_run = check["detailsUrl"]
                        .as_str()
                        .or(check["targetUrl"].as_str())
                        .and_then(run_id);
                }
                status.failed.push(name);
            }
            _ => status.pending += 1,
        }
    }
    status.comments = v["reviews"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["state"] != "APPROVED" && !s(&r["body"]).trim().is_empty())
        .count()
        + v["comments"].as_array().map_or(0, Vec::len);
    // A review that only left line comments has no body, so reviews count by their
    // state; a bare approval or a draft not yet sent has nothing to act on.
    for r in v["reviews"].as_array().into_iter().flatten() {
        let state = s(&r["state"]);
        let said = !s(&r["body"]).trim().is_empty();
        if matches!(state.as_str(), "COMMENTED" | "CHANGES_REQUESTED")
            || (state == "APPROVED" && said)
        {
            let author = s(&r["author"]["login"]);
            status.voices.push(Voice {
                trusted: trusted_author(&author, &s(&r["authorAssociation"]), &status.author),
                author,
                at: s(&r["submittedAt"]),
                review: true,
            });
        }
    }
    for c in v["comments"].as_array().into_iter().flatten() {
        if !s(&c["body"]).trim().is_empty() {
            let author = s(&c["author"]["login"]);
            status.voices.push(Voice {
                trusted: trusted_author(&author, &s(&c["authorAssociation"]), &status.author),
                author,
                at: s(&c["createdAt"]),
                review: false,
            });
        }
    }
    Some(status)
}

/// The Actions run id in a check's link, such as `.../actions/runs/123/job/456`.
pub fn run_id(url: &str) -> Option<u64> {
    let rest = url.split("/actions/runs/").nth(1)?;
    rest.split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}
