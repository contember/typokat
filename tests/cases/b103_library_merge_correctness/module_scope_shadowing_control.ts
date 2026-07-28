// Backlog 103 control: module-local library spellings shadow rather than merge or route private.
export {};

interface Array<T> {
  b103ModuleElement: T;
}
class Date {
  b103ModuleStamp(): boolean {
    return true;
  }
}

declare const localArray: Array<number>;
const localElement: number = localArray.b103ModuleElement;
const wrongLocalElement: string = localArray.b103ModuleElement; // error[TK2322]: Type 'number' is not assignable to type 'string'
const localStamp: boolean = new Date().b103ModuleStamp();
const wrongLocalStamp: string = new Date().b103ModuleStamp(); // error[TK2322]: Type 'boolean' is not assignable to type 'string'

[1, 2].b103ModuleElement; // error[TK2339]
"text".length.toFixed(0).b103ModuleStamp; // error[TK2339]
