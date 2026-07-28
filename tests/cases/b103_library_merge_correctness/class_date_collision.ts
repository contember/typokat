// Backlog 103 correctness: illegal class collisions stay library-winning without panicking.
class Date {
  b103Stamp(): number {
    return 1;
  }
}

const date: Date = new Date();
new Date().b103Stamp(); // error[TK2339]
const wrongDate: string = new Date(); // error[TK2322]
