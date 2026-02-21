use snapshot_tests::valid;

valid!(comments);
valid!(evil);

mod arrays {
    use super::valid;

    valid!(empty, "thevoid = [[[[[]]]]]");
    valid!(no_spaces, "ints = [1,2,3]");
    valid!(heterogenous, r#"mixed = [[1, 2], ["a", "b"], [1.1, 2.1]]"#);
    valid!(nested, r#"nest = [["a"], ["b"]]"#);
    valid!(ints_and_floats, "ints-and-floats = [1, 1.1]");
    valid!(
        ints_and_arrays,
        r#"arrays-and-ints =  [1, ["Arrays are not integers."]]"#
    );
    valid!(strings_and_ints, r#"strings-and-ints = ["hi", 42]"#);
    valid!(
        one,
        "[[people]]\nfirst_name = \"Bruce\"\nlast_name = \"Springsteen\""
    );
}

mod tables {
    use super::valid;

    valid!(
        implicit_and_explicit_after,
        r"
[a.b.c]
answer = 42

[a]
better = 43
"
    );
    valid!(
        implicit_and_explicit_before,
        r"
[a]
better = 43

[a.b.c]
answer = 42
"
    );
    valid!(implicit_groups, "[a.b.c]\nanswer = 42");
    valid!(implicit_array, "[[albums.songs]]\nname = \"Glory Days\"");
    valid!(
        array_many,
        r#"
[[people]]
first_name = "Bruce"
last_name = "Springsteen"

[[people]]
first_name = "Eric"
last_name = "Clapton"

[[people]]
first_name = "Bob"
last_name = "Seger"
"#
    );
    valid!(
        nested_arrays,
        r#"
[[albums]]
name = "Born to Run"

  [[albums.songs]]
  name = "Jungleland"

  [[albums.songs]]
  name = "Meeting Across the River"

[[albums]]
name = "Born in the USA"

  [[albums.songs]]
  name = "Glory Days"

  [[albums.songs]]
  name = "Dancing in the Dark"
"#
    );
    valid!(sub_empty, "[a]\n[a.b]");
    valid!(
        table_span,
        "\n[workspace]\nmembers = true\nother = false\n[workspace.foo]\nbar=3\n\n[workspace.bar]\ninline = { foo = 3 }\n\n"
    );
}

mod numbers {
    use super::valid;

    valid!(integers);
    valid!(floats);
}
