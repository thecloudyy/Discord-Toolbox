use leptos::prelude::*;

fn svg(paths: &'static [&'static str], size: u32) -> impl IntoView {
    view! {
        <svg
            width=size
            height=size
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.7"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            {paths
                .iter()
                .map(|path| view! { <path d=*path /> })
                .collect_view()}
        </svg>
    }
}

pub fn dashboard() -> impl IntoView {
    svg(
        &[
            "M4 4h6v6H4z",
            "M14 4h6v6h-6z",
            "M4 14h6v6H4z",
            "M14 14h6v6h-6z",
        ],
        19,
    )
}

pub fn badges() -> impl IntoView {
    svg(
        &[
            "M12 3.5a5 5 0 1 0 0 10 5 5 0 0 0 0-10Z",
            "M8.8 12.6 7.2 20.5l4.8-2.6 4.8 2.6-1.6-7.9",
        ],
        19,
    )
}

pub fn hypesquad() -> impl IntoView {
    svg(
        &[
            "M12 3 5 6v5.5c0 4.4 3 7.5 7 9.5 4-2 7-5.1 7-9.5V6l-7-3Z",
            "m9.3 11.8 1.9 1.9 3.7-3.8",
        ],
        19,
    )
}

pub fn logs() -> impl IntoView {
    svg(
        &[
            "M4 5.5h16v13H4z",
            "m7.5 9 2.5 2.5L7.5 14",
            "M12.5 14H17",
        ],
        19,
    )
}

pub fn copy() -> impl IntoView {
    svg(
        &[
            "M9 9h9.5v10.5H9z",
            "M6 15H4.5V4.5H15V6",
        ],
        16,
    )
}

pub fn check() -> impl IntoView {
    svg(&["m5 12.5 4.2 4.2L19 7"], 16)
}

pub fn alert() -> impl IntoView {
    svg(
        &[
            "M12 3.5 21 20H3L12 3.5Z",
            "M12 9v5",
            "M12 17h.01",
        ],
        18,
    )
}

pub fn play() -> impl IntoView {
    svg(&["M8 5.5 18 12 8 18.5v-13Z"], 16)
}

pub fn stop() -> impl IntoView {
    svg(&["M6.5 6.5h11v11h-11z"], 16)
}

pub fn restart() -> impl IntoView {
    svg(
        &[
            "M20 11a8 8 0 1 0-2.3 6.7",
            "M20 4.5V11h-6.5",
        ],
        16,
    )
}

pub fn refresh() -> impl IntoView {
    svg(
        &[
            "M20 11a8 8 0 0 0-14.8-4",
            "M4 4.5V10h5.5",
            "M4 13a8 8 0 0 0 14.8 4",
            "M20 19.5V14h-5.5",
        ],
        16,
    )
}

pub fn spark() -> impl IntoView {
    svg(
        &[
            "M12 3.5 13.8 9l5.7 1.8-5.7 1.8L12 18.2l-1.8-5.6L4.5 10.8 10.2 9 12 3.5Z",
        ],
        16,
    )
}

pub fn external() -> impl IntoView {
    svg(
        &[
            "M14 4.5h5.5V10",
            "M19.5 4.5 11 13",
            "M18 14v5.5H4.5V6H10",
        ],
        15,
    )
}

pub fn minimize() -> impl IntoView {
    svg(&["M5 12h14"], 14)
}

pub fn maximize() -> impl IntoView {
    svg(&["M5.5 5.5h13v13h-13z"], 12)
}

pub fn close() -> impl IntoView {
    svg(&["m6 6 12 12", "m18 6-12 12"], 14)
}

pub fn user() -> impl IntoView {
    svg(
        &[
            "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z",
            "M4.5 20a7.5 7.5 0 0 1 15 0",
        ],
        18,
    )
}

pub fn trash() -> impl IntoView {
    svg(
        &[
            "M4.5 7h15",
            "M9 7V4.5h6V7",
            "m6.5 7 .9 12.5h9.2L17.5 7",
        ],
        16,
    )
}
