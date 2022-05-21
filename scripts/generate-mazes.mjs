import path from "path";
import url from "url";
import fs from "fs";
import generateMaze from "generate-maze";

const __filename = url.fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const mazeCount = 300;

for (let size = 2; size < 40; size++) {
  console.log(size);

  const outputPath = path.join(__dirname, `../mazes/mazes_${size}.json`);

  const mazes = [];
  for (let i = 0; i < mazeCount; i++) {
    const maze = generateMaze(
      size,
      size,
      true,
      Math.floor((i / (mazeCount - 1)) * 1337420)
    ).flat();
    mazes.push(maze);
  }

  fs.writeFileSync(outputPath, JSON.stringify(mazes));
}
