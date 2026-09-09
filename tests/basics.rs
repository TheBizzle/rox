#[cfg(test)]
mod tests {
  use rox::vm::VM;
  use rox::vm::interpretation::Interpretation::Success;

  #[tokio::test]
  async fn can_reference_the_same_variable_300_times() {
    let code = "var x = 3; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x;
x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x;
x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x;
x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x;
x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x;
x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x;
x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x;
x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x;
x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x; x;";

    let result = VM::interpret(code.to_string());

    assert!(result == Success);
  }

  #[tokio::test]
  async fn gc_doesnt_delete_anything_critical() {
    let setup_code = "
class OuterClass {
  init() {
    this.contents = 8;
  }

  my_method() {
    return 20;
  }
}

fun simple_fn() {
  return 12;
}

var outer_outer = OuterClass();
var outer_outer_method = outer_outer.my_method();
var outer_number = 3;

{
  class InnerClass {
    init() {
      this.contents = 8;
    }

    my_method() {
      return 20;
    }
  }

  fun inner_fn() {
    fun inner_inner_fn() {
      return outer_number;
    }
    return inner_inner_fn;
  }

  var inner_inner = InnerClass();
  var inner_outer = OuterClass();

  var inner_outer_method = inner_outer.my_method;
  var inner_method = inner_inner.my_method;

  var clck = clock;

  var returned_fn = inner_fn();
  var returned_value = returned_fn();

  var number = inner_outer_method() + inner_method() + inner_inner.contents + inner_outer.contents + clck();
  var string = \"apples\";
}
";

    let after_code = "
print clock();
print outer_outer;
print outer_outer_method;
print outer_number;
print simple_fn();
print OuterClass();
print outer_outer.my_method();
";

    let mut vm = VM::init();

    let result1 = vm.interpret_partial(setup_code.to_string());
    assert!(result1 == Success);

    vm.collect_garbage();

    let result2 = vm.interpret_partial(after_code.to_string());
    assert!(result2 == Success);
  }

  #[tokio::test]
  async fn can_declare_subclass_with_for_without_ruining_the_stack() {
    let code = "
class X {}
class Y < X {}
for (var i = 0; i < 1; i = i + 1) {}
";

    let result = VM::interpret(code.to_string());

    assert!(result == Success);
  }
}
