// M5 — single-declaration interfaces (type space). Declaration merging is post-MVP.

interface Point { x: number; y: number; }

const ok: Point = { x: 1, y: 2 };           // ok
const missing: Point = { x: 1 };            // error[TK2741]: Property 'y' is missing in type
const wrong: Point = { x: 1, y: "z" };      // error[TK2322]
const excess: Point = { x: 1, y: 2, z: 3 }; // error[TK2353]: 'z' does not exist in type

const px: number = ok.x; // ok
const pmiss = ok.z;      // error[TK2339]: Property 'z' does not exist on type
