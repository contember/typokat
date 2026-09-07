declare function seedSelf<T extends { next: T }>(): T;
const seedSelfResult = seedSelf();
const seedSelfShape: { next: unknown } = seedSelfResult;
const seedSelfWrong: { next: number } = seedSelfResult; // error[TK2322]

declare function seedMutual<U extends { value: T }, T extends { value: U }>(): [U, T];
const seedMutualResult = seedMutual();
const seedMutualFirst: { value: { value: unknown } } = seedMutualResult[0];
const seedMutualSecond: { value: unknown } = seedMutualResult[1];
const seedMutualWrong: { value: number } = seedMutualResult[1]; // error[TK2322]

declare function seedRejected<T extends { next: T }>(value: T): T;
const seedRejectedResult = seedRejected(1 as const); // error[TK2345]
const seedRejectedShape: { next: 1 } = seedRejectedResult;
const seedRejectedWrong: { next: 2 } = seedRejectedResult; // error[TK2322]

interface SeedRecursiveNode { next: SeedRecursiveNode }
declare const seedReadyNode: SeedRecursiveNode;
const seedReadyResult = seedRejected(seedReadyNode);
const seedReadyIdentity: SeedRecursiveNode = seedReadyResult;
declare function seedDefault<T extends { next: T } = SeedRecursiveNode>(): T;
const seedDefaultIdentity: SeedRecursiveNode = seedDefault();

declare class SeedConstraintBox<T extends { next: T }> {
  constructor(value: T);
  value: T;
}
const seedInvalidBox = new SeedConstraintBox(1 as const); // error[TK2345]
const seedBoxShape: { next: 1 } = seedInvalidBox.value;
const seedBoxWrong: { next: 2 } = seedInvalidBox.value; // error[TK2322]

declare function seedReverse<U extends T, T extends string>(child: U, base: T): [U, T];
const seedReverseResult = seedReverse("child" as const, 1 as const); // error[TK2345]
const seedReverseChild: "child" = seedReverseResult[0];
const seedReverseBase: 1 = seedReverseResult[1]; // error[TK2322]
