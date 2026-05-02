const std = @import("std");
const zli = @import("zli");

const config = @import("../core/config.zig");
const text = @import("../core/ui/text.zig");

pub const AppContext = struct {
    args: []const [:0]const u8,
    environ_map: *std.process.Environ.Map,
};

pub const Parsed = struct {
    dirs: []const []const u8,
    excludes: []const []const u8,
    dry_run: bool,
    include_branches: bool,
    json: bool,
    min_age: u32,
    mode: []const u8,
    style: []const u8,
    recursive: bool,
    yes: bool,
    github_token: ?[]const u8,

    pub fn deinit(self: Parsed, allocator: std.mem.Allocator) void {
        allocator.free(self.dirs);
        allocator.free(self.excludes);
    }

    pub fn core(self: Parsed) config.Config {
        return .{
            .dirs = self.dirs,
            .excludes = self.excludes,
            .include_branches = self.include_branches,
            .mode = self.mode,
            .style = self.style,
            .recursive = self.recursive,
            .github_token = self.github_token,
        };
    }
};

const dry_run_flag = zli.Flag{
    .name = "dry-run",
    .description = "Preview changes without applying them",
    .type = .Bool,
    .default_value = .{ .Bool = false },
};

const exclude_flag = zli.Flag{
    .name = "exclude",
    .description = "Exclude actions by regex (repeatable)",
    .type = .String,
    .default_value = .{ .String = "" },
};

const include_branches_flag = zli.Flag{
    .name = "include-branches",
    .description = "Also check actions pinned to branches",
    .type = .Bool,
    .default_value = .{ .Bool = false },
};

const json_flag = zli.Flag{
    .name = "json",
    .description = "Output update information as machine-readable JSON",
    .type = .Bool,
    .default_value = .{ .Bool = false },
};

const min_age_flag = zli.Flag{
    .name = "min-age",
    .description = "Minimum age in days for updates",
    .type = .Int,
    .default_value = .{ .Int = 0 },
};

const mode_flag = zli.Flag{
    .name = "mode",
    .description = "Update mode: major, minor, or patch",
    .type = .String,
    .default_value = .{ .String = "major" },
};

const style_flag = zli.Flag{
    .name = "style",
    .description = "Update style: sha or preserve",
    .type = .String,
    .default_value = .{ .String = "sha" },
};

const recursive_flag = zli.Flag{
    .name = "recursive",
    .shortcut = "r",
    .description = "Recursively scan directories for YAML files",
    .type = .Bool,
    .default_value = .{ .Bool = false },
};

const yes_flag = zli.Flag{
    .name = "yes",
    .shortcut = "y",
    .description = "Skip all confirmations",
    .type = .Bool,
    .default_value = .{ .Bool = false },
};

pub fn addFlags(command: *zli.Command) !void {
    try command.addFlags(&.{
        dry_run_flag,
        exclude_flag,
        include_branches_flag,
        json_flag,
        min_age_flag,
        mode_flag,
        style_flag,
        recursive_flag,
        yes_flag,
    });

    try command.addPositionalArg(.{
        .name = "path",
        .description = "File or directory to scan. Default: .github, or . with --recursive",
        .required = false,
        .variadic = true,
    });
}

pub fn parse(ctx: zli.CommandContext) !Parsed {
    const app_context = ctx.getContextData(AppContext);

    var dirs: std.ArrayList([]const u8) = .empty;
    errdefer dirs.deinit(ctx.allocator);

    var excludes: std.ArrayList([]const u8) = .empty;
    errdefer excludes.deinit(ctx.allocator);

    var parsed = Parsed{
        .dirs = &.{},
        .excludes = &.{},
        .dry_run = ctx.flag("dry-run", bool),
        .include_branches = ctx.flag("include-branches", bool),
        .json = ctx.flag("json", bool),
        .min_age = @intCast(ctx.flag("min-age", i32)),
        .mode = ctx.flag("mode", []const u8),
        .style = ctx.flag("style", []const u8),
        .recursive = ctx.flag("recursive", bool),
        .yes = ctx.flag("yes", bool),
        .github_token = githubToken(app_context),
    };

    try collectRepeatableExcludes(ctx, app_context.args, &excludes);
    try dirs.appendSlice(ctx.allocator, ctx.positional_args);

    if (dirs.items.len == 0) {
        try dirs.append(ctx.allocator, if (parsed.recursive) "." else ".github");
    }

    try validateChoice(ctx, "mode", parsed.mode, &.{ "major", "minor", "patch" });
    try validateChoice(ctx, "style", parsed.style, &.{ "sha", "preserve" });

    parsed.dirs = try dirs.toOwnedSlice(ctx.allocator);
    parsed.excludes = try excludes.toOwnedSlice(ctx.allocator);

    return parsed;
}

fn collectRepeatableExcludes(
    ctx: zli.CommandContext,
    args: []const [:0]const u8,
    excludes: *std.ArrayList([]const u8),
) !void {
    var i: usize = 1;
    if (i < args.len and isCommandName(args[i])) i += 1;

    while (i < args.len) : (i += 1) {
        const arg: []const u8 = args[i];

        if (std.mem.startsWith(u8, arg, "--exclude=")) {
            try excludes.append(ctx.allocator, arg["--exclude=".len..]);
            continue;
        }

        if (std.mem.indexOfScalar(u8, arg, '=') != null and std.mem.startsWith(u8, arg, "--")) {
            continue;
        }

        if (std.mem.eql(u8, arg, "--exclude")) {
            i += 1;
            if (i >= args.len) return missingValue(ctx, "--exclude");
            try excludes.append(ctx.allocator, args[i]);
            continue;
        }
    }
}

fn isCommandName(arg: []const u8) bool {
    return std.mem.eql(u8, arg, "validate");
}

fn githubToken(app_context: *AppContext) ?[]const u8 {
    const token = app_context.environ_map.get("GITHUB_TOKEN") orelse return null;
    if (token.len == 0) return null;
    return token;
}

fn validateChoice(ctx: zli.CommandContext, name: []const u8, value: []const u8, choices: []const []const u8) !void {
    for (choices) |choice| {
        if (std.mem.eql(u8, value, choice)) return;
    }

    try text.writeInvalidOption(ctx.writer, name, value);
    return error.InvalidOption;
}

fn missingValue(ctx: zli.CommandContext, flag: []const u8) error{MissingFlagValue} {
    text.writeMissingFlagValue(ctx.writer, flag);
    return error.MissingFlagValue;
}
