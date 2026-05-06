const std = @import("std");

const exe_name = "actioneer";

const DistTarget = struct {
    triple: []const u8,
};

const dist_targets = [_]DistTarget{
    .{ .triple = "aarch64-macos" },
    .{ .triple = "x86_64-macos" },
    .{ .triple = "aarch64-linux-musl" },
    .{ .triple = "x86_64-linux-musl" },
    .{ .triple = "aarch64-windows-gnu" },
    .{ .triple = "x86_64-windows-gnu" },
};

const BuildModules = struct {
    zli: *std.Build.Module,
    tree_sitter: *std.Build.Module,
};

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const modules = loadModules(b, target, optimize);
    const exe = addActioneerExecutable(b, target, optimize, modules);

    b.installArtifact(exe);
    addRunStep(b, exe);
    addTestStep(b, exe);
    addDistStep(b, optimize);
}

fn loadModules(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
) BuildModules {
    const zli_dep = b.dependency("zli", .{
        .target = target,
        .optimize = optimize,
    });
    const tree_sitter_dep = b.dependency("tree_sitter", .{
        .target = target,
        .optimize = optimize,
    });
    return .{
        .zli = zli_dep.module("zli"),
        .tree_sitter = tree_sitter_dep.module("tree_sitter"),
    };
}

fn addActioneerExecutable(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
    modules: BuildModules,
) *std.Build.Step.Compile {
    const exe = b.addExecutable(.{
        .name = exe_name,
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "zli", .module = modules.zli },
                .{ .name = "tree-sitter", .module = modules.tree_sitter },
            },
        }),
    });

    configureTreeSitterYaml(exe, b);

    return exe;
}

fn configureTreeSitterYaml(exe: *std.Build.Step.Compile, b: *std.Build) void {
    exe.root_module.addCSourceFiles(.{
        .files = &.{"vendor/tree-sitter-yaml/parser.c"},
        .flags = &.{"-std=c11"},
    });
    exe.root_module.addCSourceFiles(.{
        .files = &.{"vendor/tree-sitter-yaml/scanner.cc"},
        .flags = &.{"-std=c++17"},
    });
    exe.root_module.addIncludePath(b.path("vendor/tree-sitter-yaml"));
    exe.root_module.link_libc = true;
    exe.root_module.link_libcpp = true;
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

        const modules = loadModules(b, target, optimize);
        const exe = addActioneerExecutable(b, target, optimize, modules);

        const install = b.addInstallArtifact(exe, .{
            .dest_dir = .{ .override = .{ .custom = b.fmt("dist/{s}", .{
                dist_target.triple,
            }) } },
            .dest_sub_path = b.fmt("{s}", .{artifactName(target.result.os.tag)}),
            .pdb_dir = .disabled,
            .implib_dir = .disabled,
        });

        dist_step.dependOn(&install.step);
    }
}

fn artifactName(os_tag: std.Target.Os.Tag) []const u8 {
    return if (os_tag == .windows) exe_name ++ ".exe" else exe_name;
}
