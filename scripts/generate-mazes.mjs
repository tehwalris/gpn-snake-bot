import path from "path";
import url from "url";
import fs from "fs";
import generateMaze from "generate-maze";

const __filename = url.fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const size = [25, 25];
const mazeCount = 500;
const outputPath = path.join(__dirname, "../mazes/mazes.json");

const mazes = [];
for (let i = 0; i < mazeCount; i++) {
  const maze = generateMaze(
    size[0],
    size[1],
    true,
    Math.floor((i / (mazeCount - 1)) * 1337420)
  ).flat();
  mazes.push(maze);
}

fs.writeFileSync(outputPath, JSON.stringify(mazes));
