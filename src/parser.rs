use nom::{
    branch::alt,
    bytes::{
        complete::tag,
        complete::{take_till1, take_until, take_while1},
    },
    character::complete::{char, line_ending, multispace0, space0},
    combinator::{cut, map, opt},
    error::context,
    multi::many1,
    sequence::{delimited, pair, preceded, terminated},
    IResult,
};
use std::fmt::Display;

fn parse_valid_string(i: &str) -> IResult<&str, &str> {
    let spaces_and_symbols = "\t /-_@.,%#'";
    take_while1(move |c: char| c.is_alphanumeric() || spaces_and_symbols.contains(c))(i)
}

/// Parse comments in the form of:
/// ```recp
/// /* */
/// ```
fn parse_comment(i: &str) -> IResult<&str, &str> {
    delimited(
        tag("/*"),
        map(take_until("*/"), |v: &str| v.trim()),
        preceded(tag("*/"), space0),
    )(i)
}

/// Parse curly braces delimited utf-8
///
/// ```recp
/// {salt}
/// {tomatoes}
/// ```
fn parse_curly(i: &str) -> IResult<&str, &str> {
    delimited(
        char('{'),
        map(parse_valid_string, |v| v.trim()),
        context("missing closing }", cut(char('}'))),
    )(i)
}

/// Ingredient amounts are surrounded by parenthesis
fn parse_ingredient_amount(i: &str) -> IResult<&str, &str> {
    delimited(
        tag("("),
        parse_valid_string,
        context("missing closing )", cut(tag(")"))),
    )(i)
}

/// Ingredients come in these formats:
/// ```recp
/// {quinoa}(200gr)
/// {tomatoes}(2)
/// {sweet potatoes}(2)
/// ```
fn parse_ingredient(i: &str) -> IResult<&str, (&str, Option<&str>)> {
    pair(parse_curly, opt(parse_ingredient_amount))(i)
}

/// Materials format:
/// ```recp
/// m{pot}
/// m{small jar}
/// m{stick}
/// ```
fn parse_material(i: &str) -> IResult<&str, &str> {
    preceded(tag("m"), parse_curly)(i)
}

/// Materials format:
/// ```recp
/// t{25 minutes}
/// t{10 sec}
/// ```
fn parse_timer(i: &str) -> IResult<&str, &str> {
    preceded(tag("t"), parse_curly)(i)
}

/// We separate the tokens into words
fn parse_word(i: &str) -> IResult<&str, &str> {
    let multispace = " \t\r\n";
    take_till1(move |c| multispace.contains(c))(i)
}

/// We need to identify the spaces, and use them as tokens.
/// They are useful to rebuild the recipe
fn parse_space(i: &str) -> IResult<&str, &str> {
    let multispace = " \t\r\n";
    take_while1(move |c| multispace.contains(c))(i)
}

fn parse_metadata(i: &str) -> IResult<&str, (&str, &str)> {
    preceded(
        terminated(tag(">>"), space0),
        pair(
            take_while1(|c| c != ':'),
            preceded(terminated(tag(":"), space0), take_until("\n")),
        ),
    )(i)
}

/// The backstory is separated by `---`, and it consumes till the end
/// ```recp
/// my recipe bla with {ingredient1}
/// ---
/// This recipe was given by my grandma
/// ```
fn parse_backstory(i: &str) -> IResult<&str, &str> {
    let (tail, _) = delimited(
        preceded(line_ending, multispace0),
        tag("---"),
        preceded(line_ending, multispace0),
    )(i)?;
    // We use directly the tail to return everything
    Ok(("", tail))
}

#[derive(Debug)]
pub enum Token<'a> {
    Metadata {
        key: &'a str,
        value: &'a str,
    },
    Ingredient {
        name: &'a str,
        amount: Option<&'a str>,
    },
    Timer(&'a str),
    Material(&'a str),
    Word(&'a str),
    Space(&'a str),
    Comment(&'a str),
    Backstory(&'a str),
}

impl Display for Token<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Ingredient { name, amount: _ } => write!(f, "{}", name),
            Token::Backstory(v)
            | Token::Timer(v)
            | Token::Material(v)
            | Token::Word(v)
            | Token::Space(v) => {
                write!(f, "{}", v)
            }
            Token::Metadata { key: _, value: _ } => Ok(()),
            Token::Comment(_) => Ok(()),
        }
    }
}

/// It returns a list with the parsed tokens
///
/// This function is useful if you want to build your own render on
/// top of recipe-lang
///
/// Example:
///
/// ```
/// use recipe_lang::parse;
///
/// let input = "Take the {potatoe}(1) and boil it";
/// let result = parse(input).expect("recipe could not be parsed");
///
/// println!("{result:?}");
/// ```
pub fn parse(i: &str) -> IResult<&str, Vec<Token>> {
    many1(alt((
        map(parse_metadata, |(key, value)| Token::Metadata {
            key,
            value,
        }),
        map(parse_material, |m| Token::Material(m)),
        map(parse_timer, |t| Token::Timer(t)),
        // Because ingredient doesn't have a prefix before the curly braces, e.g: `m{}`
        // it must always be parsed after timer and material
        map(parse_ingredient, |(name, amount)| Token::Ingredient {
            name,
            amount,
        }),
        map(parse_backstory, |v| Token::Backstory(v)),
        map(parse_comment, |v| Token::Comment(v)),
        map(parse_word, |v| Token::Word(v)),
        map(parse_space, |v| Token::Space(v)),
    )))(i)
}

#[cfg(test)]
mod test {
    use super::*;
    use rstest::*;

    #[rstest]
    #[case("salt", "salt")]
    #[case("sweet potato", "sweet potato")]
    #[case("ToMaToeS", "ToMaToeS")]
    #[case("1/2 lemon", "1/2 lemon")]
    #[case("my-best-sauce", "my-best-sauce")]
    #[case("1.2", "1.2")]
    #[case("1,2", "1,2")]
    #[case("1_200", "1_200")]
    #[case("@woile", "@woile")]
    #[case("10%", "10%")]
    #[case("#vegan", "#vegan")]
    #[case("mango's", "mango's")]
    fn test_parse_valid_string(#[case] input: &str, #[case] expected: &str) {
        let (_, valid_str) = parse_valid_string(input).unwrap();
        assert_eq!(valid_str, expected)
    }
    #[rstest]
    #[case("{salt}", "salt")]
    #[case("{black pepper}", "black pepper")]
    #[case("{smashed potatoes}", "smashed potatoes")]
    #[case("{15 minutes}", "15 minutes")]
    #[case("{   15 minutes  }", "15 minutes")]
    fn test_parse_curly_ok(#[case] input: &str, #[case] expected: &str) {
        let (_, content) = parse_curly(input).expect("to work");
        assert_eq!(expected, content);
    }

    #[rstest]
    #[case("{}")]
    #[case("{unclosed")]
    fn test_parse_curly_wrong(#[case] input: &str) {
        let res = parse_curly(input);
        println!("{res:?}");
        assert!(res.is_err());
        let err = res.unwrap_err();
        println!("{}", err.to_string());
    }

    #[rstest]
    #[case("(200gr)", "200gr")]
    #[case("(1/2)", "1/2")]
    #[case("(100 gr)", "100 gr")]
    #[case("(10 ml)", "10 ml")]
    #[case("(1.5 cups)", "1.5 cups")]
    fn test_parse_ingredient_amount_ok(#[case] input: &str, #[case] expected: &str) {
        let (_, content) = parse_ingredient_amount(input).expect("to work");
        assert_eq!(expected, content);
    }

    #[rstest]
    #[case("()")]
    #[case("(unclosed")]
    fn test_parse_ingredient_amount_wrong(#[case] input: &str) {
        let res = parse_ingredient_amount(input);

        println!("{res:?}");
        assert!(res.is_err());
        let err = res.unwrap_err();
    }

    #[rstest]
    #[case("{sweet potato}(200gr)", "sweet potato", Some("200gr"))]
    #[case("{sweet potato}", "sweet potato", None)]
    fn test_parse_ingredient_ok(
        #[case] input: &str,
        #[case] expected_ingredient: &str,
        #[case] expected_amount: Option<&str>,
    ) {
        let (_, (ingredient, amount)) = parse_ingredient(input).unwrap();
        assert_eq!(expected_ingredient, ingredient);
        assert_eq!(expected_amount, amount);
    }

    #[rstest]
    #[case("m{pot}", "pot")]
    #[case("m{small jar}", "small jar")]
    #[case("m{stick}", "stick")]
    #[case("m{bricks}", "bricks")]
    fn test_parse_material_ok(#[case] input: &str, #[case] expected: &str) {
        let (_, material) = parse_material(input).expect("Failed to parse material");
        assert_eq!(material, expected)
    }

    #[rstest]
    #[case("t{1 minute}", "1 minute")]
    fn test_parse_timer_ok(#[case] input: &str, #[case] expected: &str) {
        let (_, timer) = parse_timer(input).expect("Failed to parse timer");
        assert_eq!(timer, expected)
    }

    #[rstest]
    #[case(">> tags: vegan\n", ("tags", "vegan"))]
    #[case(">> key: pepe\n", ("key", "pepe"))]
    #[case(">>key: pepe\n", ("key", "pepe"))]
    #[case(">>    key: pepe\n", ("key", "pepe"))]
    #[case(">>    key:     pepe\n", ("key", "pepe"))]
    #[case(">>    key:\t\tpepe\n", ("key", "pepe"))]
    #[case(">>    key:pepe\n", ("key", "pepe"))]
    fn test_parse_metadata_ok(#[case] input: &str, #[case] expected: (&str, &str)) {
        let (_, metadata) = parse_metadata(input).expect("Failed to parse metadata");
        assert_eq!(metadata, expected)
    }

    #[rstest]
    #[case("/* */", "")]
    #[case("/* hello */", "hello")]
    #[case("/* multi\nline\ncomment */", "multi\nline\ncomment")]
    fn test_parse_comment_ok(#[case] input: &str, #[case] expected: &str) {
        let (_, comment) = parse_comment(input).expect("failed to parse comment");
        assert_eq!(comment, expected)
    }

    #[rstest]
    #[case("\n---\nwhat a backstory", "what a backstory")]
    #[case("\n   ---\nwhat a backstory", "what a backstory")]
    #[case("\n   ---\n\nwhat a backstory", "what a backstory")]
    #[case("\n   ---\n\nthis is **markdown**", "this is **markdown**")]
    #[case("\n   ---\n\nthis is [markdown](url)", "this is [markdown](url)")]
    fn test_parse_backstory_ok(#[case] input: &str, #[case] expected: &str) {
        let (_, backsotry) = parse_backstory(input).expect("failed to parse backstory");
        assert_eq!(backsotry, expected)
    }

    #[rstest]
    #[case("\n---    \nwhat a backstory")]
    fn test_parse_backstory_fail(#[case] input: &str) {
        let out = parse_backstory(input);
        assert!(out.is_err())
    }

    #[test]
    fn test_parse_ok() {
        let input = "Boil the quinoa for t{5 minutes} in a m{pot}.\nPut the boiled {quinoa}(200gr) in the base of the bowl.";
        let expected = "Boil the quinoa for 5 minutes in a pot.\nPut the boiled quinoa in the base of the bowl.";
        let (_, recipe) = parse(input).expect("parsing recipe failed");
        let fmt_recipe = recipe
            .iter()
            .fold(String::new(), |acc, val| format!("{acc}{val}"));
        println!("{}", fmt_recipe);

        assert_eq!(expected, fmt_recipe)
    }

    #[test]
    fn test_parse_meta_ok() {
        let input = ">> name: story\nBoil the quinoa for t{5 minutes} in a m{pot}.\nPut the boiled {quinoa}(200gr) in the base of the bowl.";
        let expected = "Boil the quinoa for 5 minutes in a pot.\nPut the boiled quinoa in the base of the bowl.";
        let (_, recipe) = parse(input).expect("parsing recipe failed");
        let fmt_recipe = recipe
            .iter()
            .fold(String::new(), |acc, val| format!("{acc}{val}"));
        println!("{}", fmt_recipe);

        assert_eq!(expected, fmt_recipe.trim())
    }

    #[test]
    fn test_recipe_with_comment_ok() {
        let input = "Boil the {quinoa} /* don't do it! */ for t{5 minutes}";
        let expected = "Boil the quinoa for 5 minutes";
        let (_, recipe) = parse(input).expect("parsing recipe failed");
        let fmt_recipe = recipe
            .iter()
            .fold(String::new(), |acc, val| format!("{acc}{val}"));
        println!("{}", fmt_recipe);

        assert_eq!(expected, fmt_recipe)
    }

    #[test]
    fn test_invalid_recipes() {
        let input = "this is an {invalid recipe";
        let result = parse(input);
        assert!(result.is_err());
        println!("{result:?}");
        let err = result.unwrap_err();

        println!("type: {:?}", err);
    }
}
