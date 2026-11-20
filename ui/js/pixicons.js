// Small pixel icons for the PDF viewer and the music player, drawn from
// text bitmaps ('#' = pixel) as crisp SVG rectangles in currentColor, so they
// look the same in every theme and every font.

const BITMAPS = {
  play: [
    '..#......',
    '..##.....',
    '..###....',
    '..####...',
    '..#####..',
    '..####...',
    '..###....',
    '..##.....',
    '..#......',
  ],
  pause: [
    '.##...##.',
    '.##...##.',
    '.##...##.',
    '.##...##.',
    '.##...##.',
    '.##...##.',
    '.##...##.',
    '.##...##.',
    '.##...##.',
  ],
  prev: [
    '.#.....#.',
    '.#....##.',
    '.#...###.',
    '.#..####.',
    '.#.#####.',
    '.#..####.',
    '.#...###.',
    '.#....##.',
    '.#.....#.',
  ],
  next: [
    '.#.....#.',
    '.##....#.',
    '.###...#.',
    '.####..#.',
    '.#####.#.',
    '.####..#.',
    '.###...#.',
    '.##....#.',
    '.#.....#.',
  ],
  up: [
    '.........',
    '.........',
    '....#....',
    '...###...',
    '..#####..',
    '.#######.',
    '.........',
    '.........',
    '.........',
  ],
  down: [
    '.........',
    '.........',
    '.........',
    '.#######.',
    '..#####..',
    '...###...',
    '....#....',
    '.........',
    '.........',
  ],
  volume: [
    '.........',
    '....#....',
    '...##..#.',
    '####.#..#',
    '####.#..#',
    '####.#..#',
    '...##..#.',
    '....#....',
    '.........',
  ],
  mute: [
    '.........',
    '....#....',
    '...##....',
    '####.#.#.',
    '####.##.#',
    '####.#.#.',
    '...##....',
    '....#....',
    '.........',
  ],
  repeat: [
    '......#..',
    '.#######.',
    '.#....#..',
    '.#.......',
    '.#.....#.',
    '.......#.',
    '..#....#.',
    '.#######.',
    '..#......',
  ],
  note: [
    '....####.',
    '....#..#.',
    '....#..#.',
    '....#..#.',
    '....#..#.',
    '..###.##.',
    '.########',
    '.###..###',
    '.......#.',
  ],
};

const cache = new Map();

/** SVG markup of a pixel icon (`size` in CSS pixels, a multiple of 9 looks best). */
export function pix(name, size = 18) {
  const key = `${name}:${size}`;
  if (cache.has(key)) return cache.get(key);
  const rows = BITMAPS[name] || [];
  let rects = '';
  rows.forEach((row, y) => {
    // merge runs of pixels into one rectangle per run
    let x = 0;
    while (x < row.length) {
      if (row[x] !== '#') { x++; continue; }
      let w = 1;
      while (row[x + w] === '#') w++;
      rects += `<rect x="${x}" y="${y}" width="${w}" height="1"/>`;
      x += w;
    }
  });
  const svg = `<svg class="pix" viewBox="0 0 9 9" width="${size}" height="${size}" shape-rendering="crispEdges" fill="currentColor" aria-hidden="true">${rects}</svg>`;
  cache.set(key, svg);
  return svg;
}
