async function main() {
  for (let i = 1; (i <= 15); i++) {
    if ((__imod(i, 15, "examples/fizzbuzz.mesh:3:8") === 0)) {
      __print("FizzBuzz");
    } else if ((__imod(i, 3, "examples/fizzbuzz.mesh:5:15") === 0)) {
      __print("Fizz");
    } else if ((__imod(i, 5, "examples/fizzbuzz.mesh:7:15") === 0)) {
      __print("Buzz");
    } else {
      __print(i);
    }
  }
}

main().catch(__panic);
