{
  "id": "4aabfc56",
  "title": "Set default min_release_age and wire skip_branches",
  "tags": [
    "actioneer",
    "rewrite",
    "config",
    "update"
  ],
  "status": "closed",
  "created_at": "2026-06-18T17:33:48.610Z"
}

- Set Config default `min_release_age = Some("10h".to_string())`.
- In `plan_update_candidates`, skip branch refs when `config.skip_branches` is true.
- Branch detection can reuse the same logic as `CurrentRefKind::branch`.
- Keep tests green.
