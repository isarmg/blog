use super::*;

#[tokio::test]
async fn anonymous_comments_and_guest_messages_preserve_plain_text() {
    let (_directory, app, pool) = setup().await;
    sqlx::query("INSERT INTO article(id,user_id,sort_id,label_id,article_title,article_content) VALUES(1,1,0,0,'Open','Body')")
        .execute(&pool).await.unwrap();
    for text in ["a < b", "<3", "<img src=x onerror=window.__injected=true>"] {
        for path in [
            "/api/v1/comments",
            "/api/v1/message-comments",
            "/api/v1/love-comments",
            "/api/v1/tree-hole/guest",
        ] {
            let (field, input) = match path {
                "/api/v1/tree-hole/guest" => ("message", serde_json::json!({"message":text})),
                "/api/v1/comments" => (
                    "comment_content",
                    serde_json::json!({"article_id":1,"content":text}),
                ),
                _ => ("comment_content", serde_json::json!({"content":text})),
            };
            let response = anonymous_post(&app, path, input).await;
            assert_eq!(response.status(), StatusCode::OK, "{path}: {text}");
            assert_eq!(body_json(response).await[field], text, "{path}");
        }
    }
}

#[tokio::test]
async fn public_tree_hole_returns_newest_rows_with_requested_or_default_limit() {
    let (_directory, app, pool) = setup().await;
    sqlx::query(
        "WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM seq WHERE n < 101)
         INSERT INTO tree_hole(id,message) SELECT n,printf('Message %d',n) FROM seq",
    )
    .execute(&pool)
    .await
    .unwrap();

    let read = |query: &str| {
        app.clone().oneshot(
            Request::builder()
                .uri(format!("/api/v1/tree-hole{query}"))
                .body(Body::empty())
                .unwrap(),
        )
    };
    for (query, count) in [
        ("", 100),
        ("?size=9", 9),
        ("?size=0", 1),
        ("?size=150", 100),
    ] {
        let response = read(query).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        let rows = body.as_array().unwrap();
        assert_eq!(rows.len(), count);
        for (offset, row) in rows.iter().enumerate() {
            let id = 101 - offset;
            assert_eq!(row["id"], id);
            assert_eq!(row["message"], format!("Message {id}"));
        }
    }
    for query in ["?size=not-a-number", "?page=2"] {
        let response = read(query).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn comment_reads_preserve_sources_default_sizes_and_pagination_bounds() {
    let (_directory, app, pool) = setup().await;
    sqlx::raw_sql(
        "INSERT INTO article(id,user_id,sort_id,label_id,article_title,article_content) VALUES(1,1,0,0,'Open','Body');
         INSERT INTO comment(id,source,type,parent_comment_id,floor_comment_id,comment_content) VALUES
            (1,1,'article',NULL,NULL,'Article root'),(2,1,'article',1,1,'Article reply'),
            (3,0,'message',0,NULL,'Message root'),(4,0,'message',3,3,'Message reply'),
            (5,1,'love',NULL,NULL,'Love root'),(6,1,'love',5,5,'Love reply');",
    )
    .execute(&pool)
    .await
    .unwrap();
    for (path, default_size, expected_id) in [
        ("/api/v1/comments?article_id=1", 10, 1),
        ("/api/v1/comments/1/replies?", 5, 2),
        ("/api/v1/message-comments?", 10, 3),
        ("/api/v1/message-comments/3/replies?", 5, 4),
        ("/api/v1/love-comments?", 10, 5),
        ("/api/v1/love-comments/5/replies?", 5, 6),
    ] {
        for (parameters, page, size) in [
            ("", 1, default_size),
            ("&page=-9223372036854775808&size=9223372036854775807", 1, 50),
            (
                "&page=9223372036854775807&size=-9223372036854775808",
                100_000,
                1,
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("{path}{parameters}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{path}{parameters}");
            let body = body_json(response).await;
            assert_eq!(body["page"], page, "{path}{parameters}");
            assert_eq!(body["size"], size, "{path}{parameters}");
            assert_eq!(body["total"], 1, "{path}{parameters}");
            if page == 1 {
                assert_eq!(body["items"].as_array().unwrap().len(), 1);
                assert_eq!(body["items"][0]["id"], expected_id);
            } else {
                assert_eq!(body["items"], serde_json::json!([]));
            }
        }
    }
    for path in [
        "/api/v1/comments/3/replies",
        "/api/v1/message-comments/5/replies",
        "/api/v1/love-comments/1/replies",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
}
