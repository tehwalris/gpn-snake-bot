import path from "path";
import url from "url";
import fs from "fs";
import generateMaze from "generate-maze";

const __filename = url.fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const size = [25, 25];
const mazeCount = 50;
const outputPath = path.join(__dirname, "../mazes/mazes.json");
const seed_offset = 500;

const mazes = [];
for (let i = 0; i < 500; i++) {
  const maze = generateMaze(25, 25, true, i + seed_offset).flat();
  mazes.push(maze);
}

fs.writeFileSync(outputPath, JSON.stringify(mazes));
