let dir = "core/migrations";
let gitkeep = dir + "/.gitkeep";

// Remove .gitkeep if present
if file::exists(gitkeep) {
    file::delete(gitkeep);
}
