// tsc 6.0.3 --strict: TS2300 x2, TS2320, TS2374 x2, TS2413, TS2428 x4,
// TS2411/2687 x2, TS2717 x2, and two downstream TS2322 recovery witnesses below.
interface ConstraintConflict<T extends string = "x"> {} // error[TK2428]: All declarations of 'ConstraintConflict' must have identical type parameters
interface ConstraintConflict<T extends number = 1> {} // error[TK2428]: All declarations of 'ConstraintConflict' must have identical type parameters

interface DefaultConflict<T extends string = "x"> {} // error[TK2428]: All declarations of 'DefaultConflict' must have identical type parameters
interface DefaultConflict<T extends string = "y"> {} // error[TK2428]: All declarations of 'DefaultConflict' must have identical type parameters

interface RenamedParameterConflict<T> {} // error[TK2428]: All declarations of 'RenamedParameterConflict' must have identical type parameters
interface RenamedParameterConflict<U> {} // error[TK2428]: All declarations of 'RenamedParameterConflict' must have identical type parameters

interface PropertyConflict { value: number }
interface PropertyConflict { value: string } // error[TK2717]: Subsequent property declarations must have the same type
declare const propertyConflict: PropertyConflict;
const propertyConflictWrong: boolean = propertyConflict.value; // error[TK2322]

interface DuplicateKind { entry: number } // error[TK2300]: Duplicate identifier 'entry'
interface DuplicateKind { entry(): void } // error[TK2300]: Duplicate identifier 'entry'
declare const duplicateKind: DuplicateKind;
const duplicateKindWrong: string = duplicateKind.entry; // error[TK2322]: Type 'number' is not assignable to type 'string'

interface ModifierConflict { value: number } // error[TK2687]: All declarations of 'value' must have identical modifiers
interface ModifierConflict { value?: number } // error[TK2687]: All declarations of 'value' must have identical modifiers | error[TK2717]: Subsequent property declarations must have the same type

interface IndexConflict { [key: string]: number } // error[TK2374]: Duplicate index signature for type 'string'
interface IndexConflict { [key: string]: string } // error[TK2374]: Duplicate index signature for type 'string'

interface CrossIndexConflict { [key: string]: number }
interface CrossIndexConflict { [index: number]: string } // error[TK2413]: 'number' index type 'string' is not assignable to 'string' index type 'number'

interface PropertyIndexConflict { fixed: string } // error[TK2411]: Property 'fixed' of type 'string' is not assignable to 'string' index type 'number'
interface PropertyIndexConflict { [key: string]: number }

interface NumberBase { value: number }
interface StringBase { value: string }
interface HeritageConflict extends NumberBase {} // error[TK2320]: cannot simultaneously extend types 'NumberBase' and 'StringBase'
interface HeritageConflict extends StringBase {}

interface CompatibleBaseA { left: number }
interface CompatibleBaseB { right: string }
interface CompatibleHeritage extends CompatibleBaseA {}
interface CompatibleHeritage extends CompatibleBaseB {}
const compatibleHeritage: CompatibleHeritage = { left: 1, right: "ok" };
