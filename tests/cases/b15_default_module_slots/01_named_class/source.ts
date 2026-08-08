export default class Widget { value: number = 1; }
const local: Widget = new Widget();
export const localValue: number = local.value;
