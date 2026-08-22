const directValues = [1, 2, 3]
const directHead: number = directValues.projectLocalHead()
const directMapped: number[] = directValues.map((value) => value + 1)
const directArrayConstructor: ArrayConstructor = globalThis.Array
const wrongDirectHead: string = directValues.projectLocalHead()
