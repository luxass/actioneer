const std = @import("std");
const build_options = @import("build_options");
const zli = @import("zli");

const update = @import("cmd/update.zig");
const validate = @import("cmd/validate.zig");
const version = @import("cmd/version.zig");
const check_workflows = @import("app/check_workflows.zig");
const output = @import("app/ui/output.zig");
const github = @import("core/github.zig");
const log = @import("core/log.zig");
const runtime = @import("core/runtime.zig");

pub const app_version = build_options.app_version;

pub const CommandKind = enum {
    update,
    validate,
};

pub const ProcessState = struct {
    args: []const [:0]const u8,
    environ_map: *std.process.Environ.Map,
};

pub fn build(init_options: zli.InitOptions) !*zli.Command {
    const root = try zli.Command.init(init_options, .{
        .name = "actioneer",
        .description = "Actioneer CLI",
        .version = app_version,
    }, update.run);

    try addFlags(root);
    try root.addCommands(&.{
        try validate.register(init_options),
        try version.register(init_options),
    });

    return root;
}

pub const CommandInput = struct {
    command: CommandKind,
    paths: []const []const u8,
    excludes: []const []const u8,
    recursive: bool,
    include_branches: bool,
    mode: github.UpdateMode,
    style: github.PinStyle,
    json: bool,
    dry_run: bool,
    yes: bool,
    verbose: bool,
    ci: bool,

    pub fn parse(ctx: zli.CommandContext, command: CommandKind) !CommandInput {
        var paths: std.ArrayList([]const u8) = .empty;
        errdefer paths.deinit(ctx.allocator);

        var excludes: std.ArrayList([]const u8) = .empty;
        errdefer excludes.deinit(ctx.allocator);

        const state = currentProcessState(ctx);
        const recursive = ctx.flag("recursive", bool);

        var input = CommandInput{
            .command = command,
            .paths = &.{},
            .excludes = &.{},
            .recursive = recursive,
            .include_branches = ctx.flag("include-branches", bool),
            .mode = try parseMode(ctx, ctx.flag("mode", []const u8)),
            .style = try parseStyle(ctx, ctx.flag("style", []const u8)),
            .json = ctx.flag("json", bool),
            .dry_run = ctx.flag("dry-run", bool),
            .yes = ctx.flag("yes", bool),
            .verbose = ctx.flag("verbose", bool) or envFlag(state.environ_map, "VERBOSE"),
            .ci = envFlag(state.environ_map, "CI"),
        };

        try collectRepeatableExcludes(ctx, state.args, &excludes);
        try paths.appendSlice(ctx.allocator, ctx.positional_args);

        if (paths.items.len == 0) {
            try paths.append(ctx.allocator, if (recursive) "." else ".github");
        }

        input.paths = try paths.toOwnedSlice(ctx.allocator);
        input.excludes = try excludes.toOwnedSlice(ctx.allocator);
        return input;
    }

    pub fn deinit(self: CommandInput, allocator: std.mem.Allocator) void {
        allocator.free(self.paths);
        allocator.free(self.excludes);
    }

    pub fn resolveOptions(self: CommandInput) github.ResolveOptions {
        return .{
            .excludes = self.excludes,
            .include_branches = self.include_branches,
            .mode = self.mode,
            .style = self.style,
        };
    }

    pub fn wantsJsonOutput(self: CommandInput) bool {
        return self.json;
    }

    pub fn wantsHumanOutput(self: CommandInput) bool {
        return !self.json;
    }

    pub fn wantsPreview(self: CommandInput) bool {
        return self.command == .update and self.dry_run;
    }

    pub fn shouldPrompt(self: CommandInput) bool {
        return self.command == .update and !self.json and !self.dry_run and !self.yes;
    }

    pub fn shouldAutoSelectAll(self: CommandInput) bool {
        return self.command == .update and !self.shouldPrompt();
    }
};

pub fn parseCommandInput(ctx: zli.CommandContext, command: CommandKind) !CommandInput {
    return CommandInput.parse(ctx, command) catch |err| switch (err) {
        error.InvalidOption, error.MissingFlagValue => {
            try ctx.writer.flush();
            return error.CommandFailed;
        },
        else => return err,
    };
}

pub fn initRuntime(input: CommandInput) void {
    runtime.init(input.verbose);
}

pub fn runCheck(
    allocator: std.mem.Allocator,
    ctx: zli.CommandContext,
    input: CommandInput,
) !?check_workflows.Result {
    if (input.wantsHumanOutput()) {
        try output.writeScanStart(ctx.writer, input.paths);
    }

    var diagnostics: github.Diagnostics = .{};
    const result = check_workflows.run(
        allocator,
        ctx.io,
        input.paths,
        input.recursive,
        input.resolveOptions(),
        &diagnostics,
    ) catch |err| {
        log.debug("check failed error={s} repository={s} status={?} cause={s}", .{
            @errorName(err),
            diagnostics.repository,
            diagnostics.status,
            diagnostics.cause,
        });
        try output.writeCheckError(ctx.writer, err, diagnostics);
        return null;
    };

    log.debug("check complete found_actions={d} candidates={d} sha_mismatches={d}", .{
        result.reference_count,
        result.candidates.len,
        output.shaMismatchCount(result.candidates),
    });

    if (result.reference_count == 0) {
        if (input.wantsJsonOutput()) {
            const empty: []const github.Candidate = &.{};
            try output.writeJson(ctx.writer, empty);
            return null;
        }

        try output.writeNoReferences(ctx.writer);
        return null;
    }

    if (input.wantsHumanOutput()) {
        try output.writeFoundReferences(ctx.writer, result.reference_count);
    }

    return .{
        .reference_count = result.reference_count,
        .candidates = result.candidates,
    };
}

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

fn currentProcessState(ctx: zli.CommandContext) *const ProcessState {
    return ctx.getContextData(ProcessState);
}

fn parseMode(ctx: zli.CommandContext, value: []const u8) !github.UpdateMode {
    if (std.mem.eql(u8, value, "major")) return .major;
    if (std.mem.eql(u8, value, "minor")) return .minor;
    if (std.mem.eql(u8, value, "patch")) return .patch;

    try output.writeInvalidOption(ctx.writer, "mode", value);
    return error.InvalidOption;
}

fn parseStyle(ctx: zli.CommandContext, value: []const u8) !github.PinStyle {
    if (std.mem.eql(u8, value, "sha")) return .sha;
    if (std.mem.eql(u8, value, "preserve")) return .preserve;

    try output.writeInvalidOption(ctx.writer, "style", value);
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
    output.writeMissingFlagValue(ctx.writer, flag);
    return error.MissingFlagValue;
}

fn envFlag(environ_map: *const std.process.Environ.Map, name: []const u8) bool {
    const value = environ_map.get(name) orelse return false;
    return std.ascii.eqlIgnoreCase(value, "true") or
        std.mem.eql(u8, value, "1") or
        std.ascii.eqlIgnoreCase(value, "yes") or
        std.ascii.eqlIgnoreCase(value, "on");
}
