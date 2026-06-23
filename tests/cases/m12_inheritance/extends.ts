// M12 — class inheritance (extends): a subclass inherits the base's fields and
// methods; its instance type is a superset of the base, so subclass-to-base
// assignability falls out of structural width.

class Animal {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
  speak(): string {
    return this.name;
  }
}

class Dog extends Animal {
  breed: string;
  constructor(name: string, breed: string) {
    super(name);
    this.breed = breed;
  }
}

const d = new Dog("Rex", "Lab");
const n: string = d.name;     // ok — inherited field
const br: string = d.breed;   // ok — own field
const s: string = d.speak();  // ok — inherited method
const z = d.missing;          // error[TK2339]: Property 'missing' does not exist

const a: Animal = new Dog("Rex", "Lab"); // ok — Dog is assignable to Animal
const bad: Dog = new Animal("Rex");      // error[TK2741]: Property 'breed' is missing
