const std = @import("std");

const exe_name = "actioneer";

const DistTarget = struct {
    triple: []const u8,
    exe_filename: []const u8 = exe_name,
};

const dist_targets = [_]DistTarget{
    .{ .triple = "aarch64-macos" },
    .{ .triple = "x86_64-macos" },
    .{ .triple = "aarch64-linux-musl" },
    .{ .triple = "x86_64-linux-musl" },
    .{ .triple = "aarch64-windows-gnu", .exe_filename = exe_name ++ ".exe" },
    .{ .triple = "x86_64-windows-gnu", .exe_filename = exe_name ++ ".exe" },
};

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const zli = addZliModule(b, target, optimize);
    const tree_sitter = addTreeSitterModule(b, target, optimize);
    const exe = addActioneerExecutable(b, target, optimize, zli, tree_sitter);

    b.installArtifact(exe);
    addRunStep(b, exe);
    addTestStep(b, exe);
    addDistStep(b, optimize);
}

fn addZliModule(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
) *std.Build.Module {
    const zli_dep = b.dependency("zli", .{
        .target = target,
        .optimize = optimize,
    });
    return zli_dep.module("zli");
}

fn addTreeSitterModule(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
) *std.Build.Module {
    const tree_sitter_dep = b.dependency("tree_sitter", .{
        .target = target,
        .optimize = optimize,
    });
    return tree_sitter_dep.module("tree_sitter");
}

fn addActioneerExecutable(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
    zli: *std.Build.Module,
    tree_sitter: *std.Build.Module,
) *std.Build.Step.Compile {
    const exe = b.addExecutable(.{
        .name = exe_name,
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "zli", .module = zli },
                .{ .name = "tree-sitter", .module = tree_sitter },
            },
        }),
    });

    // ---- Tree-sitter YAML grammar ----

    // C part (parser)
    exe.root_module.addCSourceFiles(.{
        .files = &.{
            "vendor/tree-sitter-yaml/parser.c",
        },
        .flags = &.{
            "-std=c11",
        },
    });

    // C++ part (scanner)
    exe.root_module.addCSourceFiles(.{
        .files = &.{
            "vendor/tree-sitter-yaml/scanner.cc",
        },
        .flags = &.{
            "-std=c++17",
        },
    });

    exe.root_module.addIncludePath(b.path("vendor/tree-sitter-yaml"));
    exe.root_module.link_libc = true;
    exe.root_module.link_libcpp = true;

    return exe;
}

fn addRunStep(b: *std.Build, exe: *std.Build.Step.Compile) void {
    const run_step = b.step("run", "Run the app");
    const run_cmd = b.addRunArtifact(exe);

    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| {
        run_cmd.addArgs(args);
    }

    run_step.dependOn(&run_cmd.step);
}

fn addTestStep(b: *std.Build, exe: *std.Build.Step.Compile) void {
    const exe_tests = b.addTest(.{ .root_module = exe.root_module });

    const run_exe_tests = b.addRunArtifact(exe_tests);

    const test_step = b.step("test", "Run tests");
    test_step.dependOn(&run_exe_tests.step);
}

fn addDistStep(b: *std.Build, optimize: std.builtin.OptimizeMode) void {
    const dist_step = b.step("dist", "Build release binaries for supported platforms");

    for (dist_targets) |dist_target| {
        const target = b.resolveTargetQuery(std.Build.parseTargetQuery(.{
            .arch_os_abi = dist_target.triple,
        }) catch @panic("invalid dist target"));

        const zli = addZliModule(b, target, optimize);
        const tree_sitter = addTreeSitterModule(b, target, optimize);
        const exe = addActioneerExecutable(b, target, optimize, zli, tree_sitter);

        const install = b.addInstallArtifact(exe, .{
            .dest_dir = .{ .override = .{ .custom = b.fmt("dist/{s}", .{
                dist_target.triple,
            }) } },
            .dest_sub_path = b.fmt("{s}", .{
                dist_target.exe_filename,
            }),
            .pdb_dir = .disabled,
            .implib_dir = .disabled,
        });

        dist_step.dependOn(&install.step);
    }
}
