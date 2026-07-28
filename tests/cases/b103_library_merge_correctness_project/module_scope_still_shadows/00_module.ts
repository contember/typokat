export {};

interface Array<T> {
  b103CrossElement: T;
}
class Date {
  b103CrossStamp(): boolean {
    return true;
  }
}

declare const moduleArray: Array<number>;
const moduleElement: number = moduleArray.b103CrossElement;
const wrongModuleElement: string = moduleArray.b103CrossElement; // error[TK2322]
const moduleStamp: boolean = new Date().b103CrossStamp();
const wrongModuleStamp: string = new Date().b103CrossStamp(); // error[TK2322]
