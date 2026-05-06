const std = @import("std");
const zli = @import("zli");

const types = @import("../core/types.zig");
const text = @import("ui.zig");

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
    mode: types.UpdateMode,
    style: types.PinStyle,
    recursive: bool,
    verbose: bool,
    yes: bool,

    pub fn deinit(self: Parsed, allocator: std.mem.Allocator) void {
        allocator.free(self.dirs);
        allocator.free(self.excludes);
    }

    pub fn scanOptions(self: Parsed) types.ScanOptions {
        return .{
            .dirs = self.dirs,
            .recursive = self.recursive,
        };
    }

    pub fn resolveOptions(self: Parsed) types.ResolveOptions {
        return .{
            .excludes = self.excludes,
            .include_branches = self.include_branches,
            .mode = self.mode,
            .style = self.style,
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
    .description = "Exclude actions containing this text (repeatable)",
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

const verbose_flag = zli.Flag{
    .name = "verbose",
    .description = "Print more detailed progress information",
    .type = .Bool,
    .default_value = .{ .Bool = false },
};

pub fn addFlags(command: *zli.Command) !void {
    try command.addFlags(&.{
        dry_run_flag,
        exclude_flag,
        include_branches_flag,
        json_flag,
        mode_flag,
        style_flag,
        recursive_flag,
        yes_flag,
        verbose_flag,
    });

    try command.addPositionalArg(.{
        .name = "path",
        .description = "File or directory to scan. Default: .github, or . with --recursive",
        .required = false,
        .variadic = true,
    });
}

pub fn parse(ctx: zli.CommandContext) !Parsed {
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
        .mode = try parseMode(ctx, ctx.flag("mode", []const u8)),
        .style = try parseStyle(ctx, ctx.flag("style", []const u8)),
        .recursive = ctx.flag("recursive", bool),
        .verbose = ctx.flag("verbose", bool),
        .yes = ctx.flag("yes", bool),
    };

    const app_context = ctx.getContextData(AppContext);
    try collectRepeatableExcludes(ctx, app_context.args, &excludes);
    try dirs.appendSlice(ctx.allocator, ctx.positional_args);

    if (dirs.items.len == 0) {
        try dirs.append(ctx.allocator, if (parsed.recursive) "." else ".github");
    }

    parsed.dirs = try dirs.toOwnedSlice(ctx.allocator);
    parsed.excludes = try excludes.toOwnedSlice(ctx.allocator);

    return parsed;
}

fn parseMode(ctx: zli.CommandContext, value: []const u8) !types.UpdateMode {
    if (std.mem.eql(u8, value, "major")) return .major;
    if (std.mem.eql(u8, value, "minor")) return .minor;
    if (std.mem.eql(u8, value, "patch")) return .patch;

    try text.writeInvalidOption(ctx.writer, "mode", value);
    return error.InvalidOption;
}

fn parseStyle(ctx: zli.CommandContext, value: []const u8) !types.PinStyle {
    if (std.mem.eql(u8, value, "sha")) return .sha;
    if (std.mem.eql(u8, value, "preserve")) return .preserve;

    try text.writeInvalidOption(ctx.writer, "style", value);
    return error.InvalidOption;
}

fn collectRepeatableExcludes(
    ctx: zli.CommandContext,
    args: []const [:0]const u8,
    excludes: *std.ArrayList([]const u8),
) !void {
    var i: usize = 1;
    if (i < args.len and std.mem.eql(u8, args[i], ctx.command.cmd_options.name)) i += 1;

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

fn missingValue(ctx: zli.CommandContext, flag: []const u8) error{MissingFlagValue} {
    text.writeMissingFlagValue(ctx.writer, flag);
    return error.MissingFlagValue;
}
