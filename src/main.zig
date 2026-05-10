const std = @import("std");
const Io = std.Io;

const cli = @import("cli.zig");

pub fn main(init: std.process.Init) !void {
    const io = init.io;

    var stdout_buffer: [1024]u8 = undefined;
    var stdout_file_writer: Io.File.Writer = .init(.stdout(), io, &stdout_buffer);
    const stdout = &stdout_file_writer.interface;
    defer stdout.flush() catch {};

    var stdin_buffer: [1024]u8 = undefined;
    var stdin_file_reader: Io.File.Reader = .init(.stdin(), io, &stdin_buffer);
    const stdin = &stdin_file_reader.interface;

    const root = try cli.build(.{
        .allocator = init.gpa,
        .io = io,
        .writer = stdout,
        .reader = stdin,
    });
    defer root.deinit();

    var process_state = cli.ProcessState{
        .args = try init.minimal.args.toSlice(init.arena.allocator()),
        .environ_map = init.environ_map,
    };

    var args = try init.minimal.args.iterateAllocator(init.gpa);
    defer args.deinit();

    root.execute(&args, .{ .data = &process_state }) catch |err| switch (err) {
        error.CommandFailed => std.process.exit(1),
        else => return err,
    };
}
