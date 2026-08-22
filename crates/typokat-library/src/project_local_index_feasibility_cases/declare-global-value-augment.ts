export {}

declare global {
	var projectLocalDeferredValue: number
	function projectLocalDeferredFunction(): number
	class ProjectLocalDeferredClass {
		value: number
	}
}
