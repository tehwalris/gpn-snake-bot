import path from "path";
import url from "url";
import fs from "fs";
import generateMaze from "generate-maze";

const __filename = url.fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const size = [25, 25];
const mazeCount = 50;
const outputPath = path.join(__dirname, "../mazes/mazes.json");

const mazes = [];
for (let i = 0; i < 50; i++) {
  mazes.push(generateMaze(25, 25, true, i));
}

fs.writeFileSync(outputPath, JSON.stringify(mazes));
