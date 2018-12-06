
struct Predicate {}
//Predicates:
//$gt, $lt, $gte, $lte
//$eq, $ne, $not, $truthy, $is, $in 
struct Filter {
    predicates: Vec<Predicate>,

}


//
//Ok how would this work?
//Top level would get called with a string of a json object that might look like:
//{query: [$filter: {gt: 0, lt: 100}]}
//focusing on the filter statement, we have 2 filters && together.
//when we match a $filter, we'll return a Box<Iterator>
//
//how do we map js values into non-generalised predicate functions?
//  $gt, $lt, $gte, $lte are Comparisons should work for Number only 
//  $is is easy, we compare it to its type
//  $eq, $ne some value. Can we do comparisons between JSValues? => yep, partialeq is implemented
//  for Value
//  $not and $truthy test if it's coercable to a bool. 
//      Null => false
//      Number(0) => false
//      Number(not 0) => true
//      String("") => false
//      String("x") => true
//      Array => true even if empty
//      Object => true even if empty

#[cfg(test)]
mod test {
    use serde_json::*;

    #[test]
    fn equality() {
        let v1 = Value::Bool(true);
        let v2 = Value::Bool(true);

        assert_eq!(v1,v2);
 
    }
}
