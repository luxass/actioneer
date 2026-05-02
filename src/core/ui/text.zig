const std = @import("std");

const github = @import("../github.zig");
const style = @import("style.zig");
const updates = @import("../updates.zig");

pub const prompt = struct {
    pub const title = "Choose action updates";
    pub const controls_summary = "Scroll the list with wheel/arrows; move selection with j/k";
    pub const footer = "scroll wheel/arrows  j/k move  <space> row  <f> file  <enter> apply  <a> all  <i> invert  <n> none  <q> cancel";
};

pub fn displayTarget(candidate: updates.Candidate) []const u8 {
    return if (candidate.next_label.len > 0) candidate.next_label else candidate.next;
}

pub fn writeScanStart(writer: *std.Io.Writer, paths: []const []const u8) !void {
    if (paths.len == 1) {
        try writer.print("{s}Scanning{s} {s}{s}{s}\n", .{ style.cyan, style.reset, style.bold, paths[0], style.reset });
    } else {
        try writer.print("{s}Scanning{s} {s}{d}{s} input paths\n", .{ style.cyan, style.reset, style.yellow, paths.len, style.reset });
    }

    for (paths) |path| {
        try writer.print("  {s}-{s} {s}\n", .{ style.gray, style.reset, path });
    }
}

pub fn writeNoReferences(writer: *std.Io.Writer) !void {
    try writer.print("{s}No action references found.{s}\n", .{ style.yellow, style.reset });
    try writer.print("{s}Hint:{s} point actioneer at a workflow file or directory with `uses:` entries.\n", .{ style.gray, style.reset });
}

pub fn writeFoundReferences(writer: *std.Io.Writer, count: usize) !void {
    try writer.print("{s}Scanned{s} {s}{d}{s} action reference", .{ style.green, style.reset, style.yellow, count, style.reset });
    if (count != 1) try writer.print("s", .{});
    try writer.print(".\n", .{});
}

pub fn writeVerifyStart(writer: *std.Io.Writer, authenticated: bool) !void {
    try writer.print("{s}Checking{s} GitHub tags and pinned SHAs", .{ style.cyan, style.reset });
    if (authenticated) {
        try writer.print(" {s}(authenticated){s}", .{ style.green, style.reset });
    }
    try writer.print("...\n", .{});
}

pub fn writeUpdateCount(writer: *std.Io.Writer, count: usize) !void {
    try writer.print("{s}Resolved{s} {s}{d}{s} available update", .{ style.green, style.reset, style.yellow, count, style.reset });
    if (count != 1) try writer.print("s", .{});
    try writer.print(".\n", .{});
}

pub fn hasShaMismatches(candidates: []const updates.Candidate) bool {
    for (candidates) |candidate| {
        if (candidate.sha_mismatch) return true;
    }
    return false;
}

pub fn shaMismatchCount(candidates: []const updates.Candidate) usize {
    var count: usize = 0;
    for (candidates) |candidate| {
        if (candidate.sha_mismatch) count += 1;
    }
    return count;
}

pub fn writeShaMismatchWarning(writer: *std.Io.Writer, candidates: []const updates.Candidate) !void {
    const count = shaMismatchCount(candidates);
    if (count == 0) return;

    try writer.print("{s}Warning:{s} {s}{d}{s} pinned SHA", .{ style.yellow, style.reset, style.yellow, count, style.reset });
    if (count != 1) try writer.print("s", .{});
    try writer.print(" {s}do not match{s} their version comments.\n", .{ style.red, style.reset });
    try writeShaMismatchDetails(writer, candidates);
}

pub fn writeShaMismatchError(writer: *std.Io.Writer, candidates: []const updates.Candidate) !void {
    const count = shaMismatchCount(candidates);
    if (count == 0) return;

    try writer.print("{s}Validation failed:{s} {s}{d}{s} pinned SHA", .{ style.red, style.reset, style.yellow, count, style.reset });
    if (count != 1) try writer.print("s", .{});
    try writer.print(" do not match their stated versions.\n", .{});
    try writeShaMismatchDetails(writer, candidates);
    try writer.print("{s}Fix:{s} update the SHA to the tag commit, or correct the version comment before trusting the reference.\n", .{ style.cyan, style.reset });
}

fn writeShaMismatchDetails(writer: *std.Io.Writer, candidates: []const updates.Candidate) !void {
    for (candidates) |candidate| {
        if (!candidate.sha_mismatch) continue;

        try writer.print("  {s}-{s} {s}{s}{s} at {s}{s}:{}{s} uses {s}{s}{s}", .{
            style.gray,
            style.reset,
            style.bold,
            candidate.action,
            style.reset,
            style.cyan,
            candidate.file,
            candidate.line,
            style.reset,
            style.red,
            candidate.current,
            style.reset,
        });
        if (candidate.version_comment.len > 0) {
            try writer.print(" but says {s}{s}{s}", .{ style.yellow, candidate.version_comment, style.reset });
        }
        if (candidate.current_ref.len > 0) {
            try writer.print("; expected {s}{s}{s}", .{ style.green, shortSha(candidate.current_ref), style.reset });
        }
        try writer.print(".\n", .{});
    }
}

fn shortSha(sha: []const u8) []const u8 {
    return sha[0..@min(12, sha.len)];
}

pub fn writeValidationComplete(writer: *std.Io.Writer) !void {
    try writer.print("{s}Verification completed.{s}\n", .{ style.green, style.reset });
}

pub fn writeValidationSummary(writer: *std.Io.Writer, references: usize, candidates: usize) !void {
    try writer.print("{s}Validated{s} {s}{d}{s} action reference", .{ style.green, style.reset, style.yellow, references, style.reset });
    if (references != 1) try writer.print("s", .{});
    try writer.print("; {s}{d}{s} update", .{ style.yellow, candidates, style.reset });
    if (candidates != 1) try writer.print("s", .{});
    try writer.print(" available.\n", .{});
}

pub fn writeNoUpdates(writer: *std.Io.Writer) !void {
    try writer.print("{s}Everything is already up to date.{s}\n", .{ style.green, style.reset });
}

pub fn writeNoSelection(writer: *std.Io.Writer) !void {
    try writer.print("{s}No updates selected.{s} No files were changed.\n", .{ style.yellow, style.reset });
}

pub fn writeSelectionUnavailable(writer: *std.Io.Writer, reason: enum { not_tty, unsupported }) !void {
    switch (reason) {
        .not_tty => try writer.print("{s}Interactive selection needs a terminal,{s} but stdin is not a TTY.\n", .{ style.yellow, style.reset }),
        .unsupported => try writer.print("{s}Interactive selection is not available{s} on this platform yet.\n", .{ style.yellow, style.reset }),
    }
    try writer.print("Use {s}--yes{s} to apply every update, {s}--dry-run{s} to preview, or {s}--json{s} for automation.\n", .{ style.cyan, style.reset, style.cyan, style.reset, style.cyan, style.reset });
}

pub fn writeSelectionCanceled(writer: *std.Io.Writer, interrupted: bool) !void {
    if (interrupted) {
        try writer.print("{s}Interrupted.{s} No files were changed.\n", .{ style.yellow, style.reset });
    } else {
        try writer.print("{s}Selection canceled.{s} No files were changed.\n", .{ style.yellow, style.reset });
    }
}

pub fn writeSelectedUpdates(writer: *std.Io.Writer, candidates: []const updates.Candidate, selected: []const usize) !void {
    try writer.print("{s}Applying{s} {s}{d}{s} selected update", .{ style.cyan, style.reset, style.yellow, selected.len, style.reset });
    if (selected.len != 1) try writer.print("s", .{});
    try writer.print(":\n", .{});

    for (selected) |index| {
        const candidate = candidates[index];
        try writer.print("  {s}-{s} {s}{s}{s}: {s}{s}{s} -> {s}{s}{s}\n", .{
            style.gray,
            style.reset,
            style.bold,
            candidate.action,
            style.reset,
            style.gray,
            candidate.current,
            style.reset,
            if (candidate.next_is_major) style.red else style.green,
            displayTarget(candidate),
            style.reset,
        });
    }
}

pub fn writeApplyComplete(writer: *std.Io.Writer, applied: usize) !void {
    try writer.print("{s}Updated{s} {s}{d}{s} workflow reference", .{ style.green, style.reset, style.yellow, applied, style.reset });
    if (applied != 1) try writer.print("s", .{});
    try writer.print(".\n", .{});
}

pub fn writePreview(writer: *std.Io.Writer, references: usize, candidates: []const updates.Candidate) !void {
    try writer.print("{s}Preview{s}: {s}{d}{s} scanned reference", .{ style.cyan, style.reset, style.yellow, references, style.reset });
    if (references != 1) try writer.print("s", .{});
    try writer.print(", {s}{d}{s} available update", .{ style.yellow, candidates.len, style.reset });
    if (candidates.len != 1) try writer.print("s", .{});
    try writer.print(".\n", .{});

    for (candidates) |candidate| {
        try writer.print("  {s}-{s} {s}{s}{s} {s}[{s}]{s}: {s}{s}{s}", .{
            style.gray,
            style.reset,
            style.bold,
            candidate.action,
            style.reset,
            style.gray,
            candidate.job,
            style.reset,
            style.yellow,
            candidate.current,
            style.reset,
        });
        if (candidate.version_comment.len > 0) {
            try writer.print(" {s}# {s}{s}", .{ style.gray, candidate.version_comment, style.reset });
        }
        if (candidate.sha_mismatch) {
            try writer.print(" {s}(SHA/comment mismatch){s}", .{ style.red, style.reset });
        }
        try writer.print(" {s}->{s} {s}{s}{s} {s}({s}:{}){s}\n", .{
            style.gray,
            style.reset,
            if (candidate.next_is_major) style.red else style.green,
            displayTarget(candidate),
            style.reset,
            style.gray,
            candidate.file,
            candidate.line,
            style.reset,
        });
    }
}

pub fn writeJson(writer: *std.Io.Writer, candidates: []const updates.Candidate) !void {
    try writer.print("{{\"updates\":[", .{});
    for (candidates, 0..) |candidate, index| {
        if (index > 0) try writer.print(",", .{});
        try writer.print(
            "{{\"action\":\"{s}\",\"job\":\"{s}\",\"current\":\"{s}\",\"versionComment\":\"{s}\",\"shaMismatch\":{},\"next\":\"{s}\",\"nextLabel\":\"{s}\",\"file\":\"{s}\",\"line\":{}}}",
            .{ candidate.action, candidate.job, candidate.current, candidate.version_comment, candidate.sha_mismatch, candidate.next, displayTarget(candidate), candidate.file, candidate.line },
        );
    }
    try writer.print("]}}\n", .{});
}

pub fn writeResolveError(writer: *std.Io.Writer, err: anyerror, diagnostics: github.Diagnostics) !void {
    try writer.print("{s}GitHub lookup failed{s}", .{ style.red, style.reset });
    if (diagnostics.repo.len > 0) {
        try writer.print(" for {s}{s}{s}", .{ style.bold, diagnostics.repo, style.reset });
    }
    try writer.print(".\n", .{});

    if (diagnostics.status) |status| {
        try writer.print("GitHub returned HTTP {s}{d}{s} ({s}).\n", .{ style.yellow, @intFromEnum(status), style.reset, @tagName(status) });
        try writeStatusHint(writer, status);
        return;
    }

    if (diagnostics.cause.len > 0) {
        try writer.print("Request error: {s}{s}{s}.\n", .{ style.yellow, diagnostics.cause, style.reset });
        try writer.print("Check network, DNS, proxy, and TLS settings before retrying.\n", .{});
        return;
    }

    if (err == error.NoTagsFound) {
        try writer.print("The repository did not expose {s}semver-like tags{s} that actioneer can compare.\n", .{ style.yellow, style.reset });
        return;
    }

    try writer.print("Unexpected resolver error: {s}{s}{s}.\n", .{ style.yellow, @errorName(err), style.reset });
}

fn writeStatusHint(writer: *std.Io.Writer, status: std.http.Status) !void {
    switch (status) {
        .forbidden => try writer.print("{s}Hint:{s} this is usually a GitHub rate limit or access restriction. Retry later, or add authenticated requests when token support lands.\n", .{ style.cyan, style.reset }),
        .not_found => try writer.print("{s}Hint:{s} the repository was not found or is not publicly accessible.\n", .{ style.cyan, style.reset }),
        .too_many_requests => try writer.print("{s}Hint:{s} GitHub is rate limiting these requests. Wait before retrying.\n", .{ style.cyan, style.reset }),
        .unauthorized => try writer.print("{s}Hint:{s} GitHub rejected the request as unauthorized.\n", .{ style.cyan, style.reset }),
        .service_unavailable, .bad_gateway, .gateway_timeout => try writer.print("{s}Hint:{s} GitHub appears temporarily unavailable. Retry shortly.\n", .{ style.cyan, style.reset }),
        else => try writer.print("{s}Hint:{s} retry later, or run with --dry-run/--json to inspect scanned references without applying changes.\n", .{ style.cyan, style.reset }),
    }
}

pub fn writeApplyError(writer: *std.Io.Writer, err: anyerror) !void {
    try writer.print("{s}Could not write selected updates:{s} {s}.\n", .{ style.red, style.reset, @errorName(err) });
    if (err == error.UpdateTargetNotFound) {
        try writer.print("A file changed after scanning, or the expected {s}`uses:`{s} reference was not found at its original line.\n", .{ style.cyan, style.reset });
        try writer.print("{s}Fix:{s} re-run actioneer so it can scan the current file contents.\n", .{ style.cyan, style.reset });
        return;
    }
    try writer.print("{s}Check:{s} some files may already have been written. Review your working tree before retrying.\n", .{ style.cyan, style.reset });
}

pub fn writeInvalidOption(writer: *std.Io.Writer, name: []const u8, value: []const u8) !void {
    try writer.print("{s}Invalid option:{s} --{s} does not accept {s}{s}{s}.\n", .{ style.red, style.reset, name, style.yellow, value, style.reset });
    try writer.print("Run {s}`actioneer --help`{s} to see accepted values.\n", .{ style.cyan, style.reset });
}

pub fn writeMissingFlagValue(writer: *std.Io.Writer, flag: []const u8) void {
    writer.print("{s}Missing value:{s} {s} expects an argument.\n", .{ style.red, style.reset, flag }) catch {};
    writer.print("Pass the value after the flag, for example {s}`{s} value`{s}.\n", .{ style.cyan, flag, style.reset }) catch {};
}

test "detect sha mismatches" {
    const candidates = [_]updates.Candidate{
        .{
            .action = "actions/checkout",
            .job = "build",
            .current = "badsha",
            .current_ref = "goodsha",
            .version_comment = "v4.2.0",
            .sha_mismatch = true,
            .next = "goodsha",
            .next_label = "v4.2.0",
            .file = ".github/workflows/ci.yml",
            .line = 4,
        },
        .{
            .action = "actions/setup-node",
            .job = "build",
            .current = "v4",
            .next = "v4",
            .file = ".github/workflows/ci.yml",
            .line = 5,
        },
    };

    try std.testing.expect(hasShaMismatches(&candidates));
    try std.testing.expectEqual(@as(usize, 1), shaMismatchCount(&candidates));
}
