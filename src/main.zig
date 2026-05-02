const std = @import("std");
const Io = std.Io;

const cli = @import("cli/root.zig");
const options = @import("cli/options.zig");

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

    var app_context = options.AppContext{
        .args = try init.minimal.args.toSlice(init.arena.allocator()),
        .environ_map = init.environ_map,
    };

    var args = try init.minimal.args.iterateAllocator(init.gpa);
    defer args.deinit();

    try root.execute(&args, .{ .data = &app_context });
}
