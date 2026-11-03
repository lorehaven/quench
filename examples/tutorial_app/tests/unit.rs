use std::rc::Rc;

use quench_example_tutorial_app::{Item, add_item, list_items, pantry_view, set_bought, store};
use quench_fullstack::ServerError;
use quench_ui::prelude::*;

// The store is one list for the process, so one test uses it (tests run in parallel).
#[quench_test::test]
async fn server_functions_are_plain_async_functions() {
    store::clear();
    let milk = add_item("  milk ".into()).await.unwrap();
    assert_eq!(
        milk,
        Item {
            id: 1,
            name: "milk".into(),
            bought: false
        }
    );
    assert!(set_bought(1, true).await.unwrap().bought);
    assert_eq!(list_items().await.unwrap().len(), 1);

    // Bad input is refused, naming the field.
    assert!(matches!(
        add_item("   ".into()).await,
        Err(ServerError::Validation { .. })
    ));
    assert!(matches!(
        add_item("x".repeat(61)).await,
        Err(ServerError::Validation { .. })
    ));
    assert!(matches!(
        set_bought(99, true).await,
        Err(ServerError::NotFound(_))
    ));
    assert_eq!(list_items().await.unwrap().len(), 1, "nothing was added");
}

#[quench_test::test]
fn the_page_shows_the_list_it_is_given() {
    let renderer = Rc::new(TestRenderer::new());
    let root = renderer.root();
    let items = vec![
        Item {
            id: 1,
            name: "milk".into(),
            bought: false,
        },
        Item {
            id: 2,
            name: "bread".into(),
            bought: true,
        },
    ];
    let (mounted, scope) = create_root(|| mount(&renderer, &root, pantry_view(items)));
    assert!(renderer.text().contains("milk"));
    assert!(renderer.text().contains("bread"));
    assert_eq!(renderer.find_by_tag("li").len(), 2);
    mounted.unmount();
    scope.dispose();
}

#[quench_test::test]
fn an_empty_name_is_refused_before_the_server_is_asked() {
    let renderer = Rc::new(TestRenderer::new());
    let root = renderer.root();
    let (mounted, scope) = create_root(|| mount(&renderer, &root, pantry_view(Vec::new())));
    let form = renderer.find_by_tag("form")[0];
    renderer.fire(form, &Event::Submit);
    assert!(renderer.text().contains("write something"));
    assert!(renderer.find_by_tag("li").is_empty());
    mounted.unmount();
    scope.dispose();
}

quench_test::main!();
