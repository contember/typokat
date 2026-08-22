interface Array<T> {
	projectLocalOnlyA(): T
}

const projectAValues = [1, 2, 3]
const projectAValue: number = projectAValues.projectLocalOnlyA()
projectAValues.projectLocalOnlyB()
