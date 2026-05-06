const zli = @import("zli");
const update = @import("cmd/update.zig");

pub fn build(init_options: zli.InitOptions) !*zli.Command {
    return update.register(init_options);
}
