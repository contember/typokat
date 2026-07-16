// tsc 6.0.3 --strict --target es2025: TS2322 x2 below. The private merge retains
// the original Array method surface as well as the augmentation.
const collisionValue: number = [1, 2].b14Collision();
const wrongCollisionValue: string = [1, 2].b14Collision(); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wrongCollisionMap: string[] = [1, 2].map((value) => value + 1); // error[TK2322]
