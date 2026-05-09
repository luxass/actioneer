const std = @import("std");

const github = @import("../../core/github.zig");
pub const styles = @import("styles.zig");

pub fn writeScanStart(writer: *std.Io.Writer, paths: []const []const u8) !void {
    if (paths.len == 1) {
        try writer.print("{s}Scanning{s} {s}{s}{s}\n", .{ styles.CYAN, styles.RESET, styles.BOLD, paths[0], styles.RESET });
    } else {
        try writer.print("{s}Scanning{s} {s}{d}{s} input paths\n", .{ styles.CYAN, styles.RESET, styles.YELLOW, paths.len, styles.RESET });
    }

    for (paths) |path| {
        try writer.print("  {s}-{s} {s}\n", .{ styles.BRIGHT_BLACK, styles.RESET, path });
    }
}

pub fn writeNoReferences(writer: *std.Io.Writer) !void {
    try writer.print("{s}No action references found.{s}\n", .{ styles.YELLOW, styles.RESET });
    try writer.print("{s}Hint:{s} point actioneer at a workflow file or directory with `uses:` entries.\n", .{ styles.BRIGHT_BLACK, styles.RESET });
}

pub fn writeFoundReferences(writer: *std.Io.Writer, count: usize) !void {
    try writer.print("{s}Scanned{s} {s}{d}{s} action reference", .{ styles.GREEN, styles.RESET, styles.YELLOW, count, styles.RESET });
    if (count != 1) try writer.print("s", .{});
    try writer.print(".\n", .{});
}

pub fn writeUpdateCount(writer: *std.Io.Writer, count: usize) !void {
    try writer.print("{s}Resolved{s} {s}{d}{s} available update", .{ styles.GREEN, styles.RESET, styles.YELLOW, count, styles.RESET });
    if (count != 1) try writer.print("s", .{});
    try writer.print(".\n", .{});
}

pub fn hasShaMismatches(candidates: []const github.Candidate) bool {
    for (candidates) |candidate| {
        if (candidate.hasShaMismatch()) return true;
    }
    return false;
}

pub fn shaMismatchCount(candidates: []const github.Candidate) usize {
    var count: usize = 0;
    for (candidates) |candidate| {
        if (candidate.hasShaMismatch()) count += 1;
    }
    return count;
}

pub fn writeShaMismatchWarning(writer: *std.Io.Writer, candidates: []const github.Candidate) !void {
    const count = shaMismatchCount(candidates);
    if (count == 0) return;

    try writer.print("{s}Warning:{s} {s}{d}{s} pinned SHA", .{ styles.YELLOW, styles.RESET, styles.YELLOW, count, styles.RESET });
    if (count != 1) try writer.print("s", .{});
    try writer.print(" {s}do not match{s} their version comments.\n", .{ styles.RED, styles.RESET });
    try writeShaMismatchDetails(writer, candidates);
}

pub fn writeShaMismatchError(writer: *std.Io.Writer, candidates: []const github.Candidate) !void {
    const count = shaMismatchCount(candidates);
    if (count == 0) return;

    try writer.print("{s}Validation failed:{s} {s}{d}{s} pinned SHA", .{ styles.RED, styles.RESET, styles.YELLOW, count, styles.RESET });
    if (count != 1) try writer.print("s", .{});
    try writer.print(" do not match their stated versions.\n", .{});
    try writeShaMismatchDetails(writer, candidates);
    try writer.print("{s}Fix:{s} update the SHA to the tag commit, or correct the version comment before trusting the reference.\n", .{ styles.CYAN, styles.RESET });
}

fn writeShaMismatchDetails(writer: *std.Io.Writer, candidates: []const github.Candidate) !void {
    for (candidates) |candidate| {
        if (!candidate.hasShaMismatch()) continue;

        try writer.print("  {s}-{s} {s}{s}{s} at {s}{s}:{}{s} uses {s}{s}{s}", .{
            styles.BRIGHT_BLACK,
            styles.RESET,
            styles.BOLD,
            candidate.action,
            styles.RESET,
            styles.CYAN,
            candidate.file,
            candidate.line,
            styles.RESET,
            styles.RED,
            candidate.current,
            styles.RESET,
        });
        if (candidate.hasVersionComment()) {
            try writer.print(" but says {s}{s}{s}", .{ styles.YELLOW, candidate.version_comment, styles.RESET });
        }
        if (candidate.hasCurrentRef()) {
            try writer.print("; expected {s}{s}{s}", .{ styles.GREEN, shortSha(candidate.current_ref), styles.RESET });
        }
        try writer.print(".\n", .{});
    }
}

fn shortSha(sha: []const u8) []const u8 {
    return sha[0..@min(12, sha.len)];
}

pub fn writeValidationSummary(writer: *std.Io.Writer, references: usize, candidates: usize) !void {
    try writer.print("{s}Validated{s} {s}{d}{s} action reference", .{ styles.GREEN, styles.RESET, styles.YELLOW, references, styles.RESET });
    if (references != 1) try writer.print("s", .{});
    try writer.print("; {s}{d}{s} update", .{ styles.YELLOW, candidates, styles.RESET });
    if (candidates != 1) try writer.print("s", .{});
    try writer.print(" available.\n", .{});
}

pub fn writeSelectionUnavailable(writer: *std.Io.Writer, reason: enum { not_tty, unsupported }) !void {
    switch (reason) {
        .not_tty => try writer.print("{s}Interactive selection needs a terminal,{s} but stdin is not a TTY.\n", .{ styles.YELLOW, styles.RESET }),
        .unsupported => try writer.print("{s}Interactive selection is not available{s} on this platform yet.\n", .{ styles.YELLOW, styles.RESET }),
    }
    try writer.print("Use {s}--yes{s} to apply every update, {s}--dry-run{s} to preview, or {s}--json{s} for automation.\n", .{ styles.CYAN, styles.RESET, styles.CYAN, styles.RESET, styles.CYAN, styles.RESET });
}

pub fn writeSelectionCanceled(writer: *std.Io.Writer, interrupted: bool) !void {
    if (interrupted) {
        try writer.print("{s}Interrupted.{s} No files were changed.\n", .{ styles.YELLOW, styles.RESET });
    } else {
        try writer.print("{s}Selection canceled.{s} No files were changed.\n", .{ styles.YELLOW, styles.RESET });
    }
}

pub fn writeSelectedUpdates(writer: *std.Io.Writer, candidates: []const github.Candidate, selected: []const usize) !void {
    try writer.print("{s}Applying{s} {s}{d}{s} selected update", .{ styles.CYAN, styles.RESET, styles.YELLOW, selected.len, styles.RESET });
    if (selected.len != 1) try writer.print("s", .{});
    try writer.print(":\n", .{});

    for (selected) |index| {
        const candidate = candidates[index];
        try writer.print("  {s}-{s} {s}{s}{s}: {s}{s}{s} -> {s}{s}{s}\n", .{
            styles.BRIGHT_BLACK,
            styles.RESET,
            styles.BOLD,
            candidate.action,
            styles.RESET,
            styles.BRIGHT_BLACK,
            candidate.current,
            styles.RESET,
            if (candidate.isMajorUpdate()) styles.RED else styles.GREEN,
            candidate.displayTarget(),
            styles.RESET,
        });
    }
}

pub fn writeApplyComplete(writer: *std.Io.Writer, applied: usize) !void {
    try writer.print("{s}Updated{s} {s}{d}{s} workflow reference", .{ styles.GREEN, styles.RESET, styles.YELLOW, applied, styles.RESET });
    if (applied != 1) try writer.print("s", .{});
    try writer.print(".\n", .{});
}

pub fn writePreview(writer: *std.Io.Writer, references: usize, candidates: []const github.Candidate) !void {
    try writer.print("{s}Preview{s}: {s}{d}{s} scanned reference", .{ styles.CYAN, styles.RESET, styles.YELLOW, references, styles.RESET });
    if (references != 1) try writer.print("s", .{});
    try writer.print(", {s}{d}{s} available update", .{ styles.YELLOW, candidates.len, styles.RESET });
    if (candidates.len != 1) try writer.print("s", .{});
    try writer.print(".\n", .{});

    for (candidates) |candidate| {
        try writer.print("  {s}-{s} {s}{s}{s} {s}[{s}]{s}: {s}{s}{s}", .{
            styles.BRIGHT_BLACK,
            styles.RESET,
            styles.BOLD,
            candidate.action,
            styles.RESET,
            styles.BRIGHT_BLACK,
            candidate.job,
            styles.RESET,
            styles.YELLOW,
            candidate.current,
            styles.RESET,
        });
        if (candidate.hasVersionComment()) {
            try writer.print(" {s}# {s}{s}", .{ styles.BRIGHT_BLACK, candidate.version_comment, styles.RESET });
        }
        if (candidate.hasShaMismatch()) {
            try writer.print(" {s}(SHA/comment mismatch){s}", .{ styles.RED, styles.RESET });
        }
        try writer.print(" {s}->{s} {s}{s}{s} {s}({s}:{}){s}\n", .{
            styles.BRIGHT_BLACK,
            styles.RESET,
            if (candidate.isMajorUpdate()) styles.RED else styles.GREEN,
            candidate.displayTarget(),
            styles.RESET,
            styles.BRIGHT_BLACK,
            candidate.file,
            candidate.line,
            styles.RESET,
        });
    }
}

pub fn writeJson(writer: *std.Io.Writer, candidates: []const github.Candidate) !void {
    var json = std.json.Stringify{ .writer = writer, .options = .{} };
    try json.beginObject();
    try json.objectField("updates");
    try json.beginArray();

    for (candidates) |candidate| {
        try json.beginObject();
        try json.objectField("action");
        try json.write(candidate.action);
        try json.objectField("job");
        try json.write(candidate.job);
        try json.objectField("current");
        try json.write(candidate.current);
        try json.objectField("versionComment");
        try json.write(candidate.version_comment);
        try json.objectField("shaMismatch");
        try json.write(candidate.hasShaMismatch());
        try json.objectField("next");
        try json.write(candidate.next);
        try json.objectField("nextLabel");
        try json.write(candidate.displayTarget());
        try json.objectField("file");
        try json.write(candidate.file);
        try json.objectField("line");
        try json.write(candidate.line);
        try json.endObject();
    }

    try json.endArray();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeResolveError(writer: *std.Io.Writer, err: anyerror, diagnostics: github.Diagnostics) !void {
    try writer.print("{s}GitHub lookup failed{s}", .{ styles.RED, styles.RESET });
    if (diagnostics.repository.len > 0) {
        try writer.print(" for {s}{s}{s}", .{ styles.BOLD, diagnostics.repository, styles.RESET });
    }
    try writer.print(".\n", .{});

    if (diagnostics.status) |status| {
        try writer.print("GitHub returned HTTP {s}{d}{s} ({s}).\n", .{ styles.YELLOW, @intFromEnum(status), styles.RESET, @tagName(status) });
        try writeStatusHint(writer, status);
        return;
    }

    if (diagnostics.cause.len > 0) {
        try writer.print("Request error: {s}{s}{s}.\n", .{ styles.YELLOW, diagnostics.cause, styles.RESET });
        try writer.print("Check network, DNS, proxy, and TLS settings before retrying.\n", .{});
        return;
    }

    if (err == error.NoTagsFound) {
        try writer.print("The repository did not expose {s}semver-like tags{s} that actioneer can compare.\n", .{ styles.YELLOW, styles.RESET });
        return;
    }

    try writer.print("Unexpected resolver error: {s}{s}{s}.\n", .{ styles.YELLOW, @errorName(err), styles.RESET });
}

pub const writeCheckError = writeResolveError;

fn writeStatusHint(writer: *std.Io.Writer, status: std.http.Status) !void {
    switch (status) {
        .forbidden => try writer.print("{s}Hint:{s} this is usually a GitHub rate limit or access restriction. Retry later, or add authenticated requests when token support lands.\n", .{ styles.CYAN, styles.RESET }),
        .not_found => try writer.print("{s}Hint:{s} the repository was not found or is not publicly accessible.\n", .{ styles.CYAN, styles.RESET }),
        .too_many_requests => try writer.print("{s}Hint:{s} GitHub is rate limiting these requests. Wait before retrying.\n", .{ styles.CYAN, styles.RESET }),
        .unauthorized => try writer.print("{s}Hint:{s} GitHub rejected the request as unauthorized.\n", .{ styles.CYAN, styles.RESET }),
        .service_unavailable, .bad_gateway, .gateway_timeout => try writer.print("{s}Hint:{s} GitHub appears temporarily unavailable. Retry shortly.\n", .{ styles.CYAN, styles.RESET }),
        else => try writer.print("{s}Hint:{s} retry later, or run with --dry-run/--json to inspect scanned references without applying changes.\n", .{ styles.CYAN, styles.RESET }),
    }
}

pub fn writeApplyError(writer: *std.Io.Writer, err: anyerror) !void {
    try writer.print("{s}Could not write selected updates:{s} {s}.\n", .{ styles.RED, styles.RESET, @errorName(err) });
    if (err == error.UpdateTargetNotFound) {
        try writer.print("A file changed after scanning, or the expected {s}`uses:`{s} reference was not found at its original line.\n", .{ styles.CYAN, styles.RESET });
        try writer.print("{s}Fix:{s} re-run actioneer so it can scan the current file contents.\n", .{ styles.CYAN, styles.RESET });
        return;
    }
    try writer.print("{s}Check:{s} some files may already have been written. Review your working tree before retrying.\n", .{ styles.CYAN, styles.RESET });
}

pub fn writeInvalidOption(writer: *std.Io.Writer, name: []const u8, value: []const u8) !void {
    try writer.print("{s}Invalid option:{s} --{s} does not accept {s}{s}{s}.\n", .{ styles.RED, styles.RESET, name, styles.YELLOW, value, styles.RESET });
    try writer.print("Run {s}`actioneer --help`{s} to see accepted values.\n", .{ styles.CYAN, styles.RESET });
}

pub fn writeMissingFlagValue(writer: *std.Io.Writer, flag: []const u8) void {
    writer.print("{s}Missing value:{s} {s} expects an argument.\n", .{ styles.RED, styles.RESET, flag }) catch {};
    writer.print("Pass the value after the flag, for example {s}`{s} value`{s}.\n", .{ styles.CYAN, flag, styles.RESET }) catch {};
}

test "detect sha mismatches" {
    const candidates = [_]github.Candidate{
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
            .ref_start = 0,
            .ref_end = 0,
        },
        .{
            .action = "actions/setup-node",
            .job = "build",
            .current = "v4",
            .next = "v4",
            .file = ".github/workflows/ci.yml",
            .line = 5,
            .ref_start = 0,
            .ref_end = 0,
        },
    };

    try std.testing.expect(hasShaMismatches(&candidates));
    try std.testing.expectEqual(@as(usize, 1), shaMismatchCount(&candidates));
}

test "write json escapes strings" {
    var out = std.Io.Writer.Allocating.init(std.testing.allocator);
    defer out.deinit();

    const candidates = [_]github.Candidate{
        .{
            .action = "owner/repo\"quoted",
            .job = "build\njob",
            .current = "v1",
            .next = "v2",
            .next_label = "v2",
            .file = ".github/workflows/ci.yml",
            .line = 7,
            .ref_start = 0,
            .ref_end = 0,
        },
    };

    try writeJson(&out.writer, &candidates);
    try std.testing.expectEqualStrings(
        "{\"updates\":[{\"action\":\"owner/repo\\\"quoted\",\"job\":\"build\\njob\",\"current\":\"v1\",\"versionComment\":\"\",\"shaMismatch\":false,\"next\":\"v2\",\"nextLabel\":\"v2\",\"file\":\".github/workflows/ci.yml\",\"line\":7}]}\n",
        out.written(),
    );
}
