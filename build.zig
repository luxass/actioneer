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

    const mod = addActioneerModule(b, target);
    const zli = addZliModule(b, target, optimize);
    const exe = addActioneerExecutable(b, target, optimize, mod, zli);

    b.installArtifact(exe);
    addRunStep(b, exe);
    addTestStep(b, mod, exe);
    addDistStep(b, optimize);
}

fn addActioneerModule(b: *std.Build, target: std.Build.ResolvedTarget) *std.Build.Module {
    return b.addModule(exe_name, .{
        .root_source_file = b.path("src/root.zig"),
        .target = target,
    });
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

fn addActioneerExecutable(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
    mod: *std.Build.Module,
    zli: *std.Build.Module,
) *std.Build.Step.Compile {
    return b.addExecutable(.{
        .name = exe_name,
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = exe_name, .module = mod },
                .{ .name = "zli", .module = zli },
            },
        }),
    });
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

fn addTestStep(b: *std.Build, mod: *std.Build.Module, exe: *std.Build.Step.Compile) void {
    const mod_tests = b.addTest(.{ .root_module = mod });
    const exe_tests = b.addTest(.{ .root_module = exe.root_module });

    const run_mod_tests = b.addRunArtifact(mod_tests);
    const run_exe_tests = b.addRunArtifact(exe_tests);

    const test_step = b.step("test", "Run tests");
    test_step.dependOn(&run_mod_tests.step);
    test_step.dependOn(&run_exe_tests.step);
}

fn addDistStep(b: *std.Build, optimize: std.builtin.OptimizeMode) void {
    const dist_step = b.step("dist", "Build release binaries for supported platforms");

    for (dist_targets) |dist_target| {
        const target = b.resolveTargetQuery(std.Build.parseTargetQuery(.{
            .arch_os_abi = dist_target.triple,
        }) catch @panic("invalid dist target"));

        const mod = addActioneerModule(b, target);
        const zli = addZliModule(b, target, optimize);
        const exe = addActioneerExecutable(b, target, optimize, mod, zli);

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
