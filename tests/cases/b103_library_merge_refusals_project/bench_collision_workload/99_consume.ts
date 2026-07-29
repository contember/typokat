// The consumer half of the benchmark collision workload; see 00_augment.ts.
const collisionNumber: number = [1, 2, 3].fullLibBenchFirst();
const collisionMapped: number[] = [1, 2, 3].map((value) => value + 1);
const collisionDom: HTMLDivElement = document.createElement("div");

void [collisionNumber, collisionMapped, collisionDom];
