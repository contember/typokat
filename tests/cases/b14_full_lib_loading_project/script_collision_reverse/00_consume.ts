// Same semantic demands as the forward project, before the augmentation in input order.
const collisionValue: number = [1, 2].b14Collision();
const wrongCollisionValue: string = [1, 2].b14Collision(); // error[TK2322]: Type 'number' is not assignable to type 'string'
const wrongCollisionMap: string[] = [1, 2].map((value) => value + 1); // error[TK2322]
