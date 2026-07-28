const collisionNumber: number = [1, 2, 3].fullLibBenchFirst();
const wrongCollisionNumber: string = [1, 2, 3].fullLibBenchFirst(); // error[TK2322]
const collisionMapped: number[] = [1, 2, 3].map((value) => value + 1);
const collisionDom: HTMLDivElement = document.createElement("div");

void [collisionNumber, collisionMapped, collisionDom];
