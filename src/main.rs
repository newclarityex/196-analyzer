#[macro_use]
extern crate dotenv_codegen;

use plotly::{Bar, ImageFormat, Plot};
use roux::{response::BasicThing, submission::SubmissionData, util::FeedOption, Reddit, Subreddit};
use std::collections::{HashMap, HashSet};

type SubmissionsVec = Vec<BasicThing<SubmissionData>>;

#[derive(PartialEq)]
enum PostType {
    Hot,
    Latest,
    Top,
}

async fn collect_posts(subreddit: &Subreddit, count: u32, post_type: PostType) -> SubmissionsVec {
    // Collect 100 every minute
    let mut posts: SubmissionsVec = Vec::new();
    let mut remaining = count;
    let mut after: Option<String> = None;
    let mut existing_ids = HashSet::new();

    while remaining > 0 {
        let limit = std::cmp::min(100, remaining);
        let mut options = FeedOption::new().limit(limit).count(posts.len() as u32);

        if let Some(valid_after) = after {
            options = options.after(valid_after.as_str());
        }

        if post_type == PostType::Top {
            options = options.period(roux::util::TimePeriod::ThisMonth);
        }

        let new_posts = match post_type {
            PostType::Hot => {
                subreddit
                    .hot(limit, Some(options))
                    .await
                    .unwrap()
                    .data
                    .children
            }
            PostType::Latest => {
                subreddit
                    .latest(limit, Some(options))
                    .await
                    .unwrap()
                    .data
                    .children
            }
            PostType::Top => {
                subreddit
                    .top(limit, Some(options))
                    .await
                    .unwrap()
                    .data
                    .children
            }
        };

        remaining = remaining.saturating_sub(new_posts.len() as u32);

        for post in new_posts {
            let id = post.data.id.to_owned();
            println!("Added post: {}", post.data.title);
            if existing_ids.contains(&id) {
                println!("duplicate id: {}", id);
            } else {
                existing_ids.insert(id);
                posts.push(post);
            }
        }

        let new_after: String = "t3_".to_owned() + &posts.last().unwrap().data.id.to_owned();

        after = Some(new_after);

        println!("after: {:?}", after);

        println!("{} posts remaining\n", remaining);
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }

    // remove any extra posts
    posts.truncate(count as usize);

    posts
}

fn process_posts_flair(posts: &SubmissionsVec, include_flairless: bool) -> HashMap<String, i32> {
    let mut results: HashMap<String, i32> = HashMap::new();

    posts.iter().for_each(|post| {
        if (!include_flairless) && (post.data.link_flair_text.is_none()) {
            return;
        }

        let flair = post
            .data
            .link_flair_text
            .clone()
            .unwrap_or("None".to_string());

        if let std::collections::hash_map::Entry::Vacant(e) = results.entry(flair.clone()) {
            e.insert(1);
        } else {
            let count = results.get_mut(&flair).unwrap();
            *count += 1;
        }
    });

    results
}

fn process_posts_nsfw(posts: &SubmissionsVec) -> [i32; 2] {
    let mut results = [0, 0];

    posts.iter().for_each(|post| {
        if post.data.over_18 {
            results[0] += 1;
        } else {
            results[1] += 1;
        }
    });

    results
}

fn collect_data(keys: &[String], data: HashMap<String, i32>) -> Vec<i32> {
    keys.iter()
        .map(|flair| *data.get(flair).unwrap_or(&0))
        .collect()
}

fn print_percentages(data: &HashMap<String, i32>) {
    let total: i32 = data.values().sum();

    data.iter().for_each(|(flair, count)| {
        let percentage = (*count as f32 / total as f32) * 100.0;
        println!("{}: {:.2}%", flair, percentage);
    });
}

fn print_percentages_nsfw(data: &[i32; 2]) {
    let total: i32 = data.iter().sum();

    println!("NSFW: {:.2}%", (data[0] as f32 / total as f32) * 100.0);
    println!("SFW: {:.2}%", (data[1] as f32 / total as f32) * 100.0);
}

#[tokio::main]
async fn main() {
    let user_agent = dotenv!("USER_AGENT");
    let client_id = dotenv!("CLIENT_ID");
    let client_secret = dotenv!("CLIENT_SECRET");
    let username = dotenv!("REDDIT_USERNAME");
    let password = dotenv!("REDDIT_PASSWORD");

    let client = Reddit::new(user_agent, client_id, client_secret)
        .username(username)
        .password(password)
        .login()
        .await;
    let me = client.unwrap();

    // Fetch hot posts from subreddit
    let subreddit = Subreddit::new_oauth("196", &me.client);

    let count = 1000;

    println!("Fetching {} posts from r/{}", count, subreddit.name);

    let hot = collect_posts(&subreddit, count, PostType::Hot).await;
    let hot_data_flair = process_posts_flair(&hot, true);

    let latest = collect_posts(&subreddit, count, PostType::Latest).await;
    let latest_data_flair = process_posts_flair(&latest, true);

    let top = collect_posts(&subreddit, count, PostType::Top).await;
    let top_data_flair = process_posts_flair(&top, true);

    let mut all_flairs: Vec<String> = Vec::new();
    all_flairs.extend(hot_data_flair.keys().cloned());
    all_flairs.extend(latest_data_flair.keys().cloned());
    all_flairs.extend(top_data_flair.keys().cloned());

    // Plot flair data
    let hot_data = collect_data(&all_flairs, hot_data_flair.clone());
    let hot_trace = Bar::new(all_flairs.clone(), hot_data).name("Hot");

    let latest_data = collect_data(&all_flairs, latest_data_flair.clone());
    let latest_trace = Bar::new(all_flairs.clone(), latest_data).name("Latest");

    let top_data = collect_data(&all_flairs, top_data_flair.clone());
    let top_trace = Bar::new(all_flairs.clone(), top_data).name("Top");

    let mut flair_plot = Plot::new();
    flair_plot.add_trace(hot_trace);
    flair_plot.add_trace(latest_trace);
    flair_plot.add_trace(top_trace);

    flair_plot.write_image("flairs.png", ImageFormat::PNG, 800, 600, 1.0);

    println!("--- Flair Post Data ---");
    println!("Hot Flair Percentages:");
    print_percentages(&hot_data_flair);
    println!("\nLatest Flair Percentages:");
    print_percentages(&latest_data_flair);
    println!("\nTop Flair Percentages:");
    print_percentages(&top_data_flair);

    // Plot No Flairless
    let hot_data_no_flairless = process_posts_flair(&hot, false);
    let latest_data_no_flairless = process_posts_flair(&latest, false);
    let top_data_no_flairless = process_posts_flair(&top, false);
    let all_flairs_no_flairless: Vec<String> = all_flairs
        .iter()
        .filter(|flair| flair != &&"None".to_string())
        .cloned()
        .collect();

    let mut no_flairless_plot = Plot::new();
    let hot_data = collect_data(&all_flairs_no_flairless, hot_data_no_flairless);
    let hot_trace = Bar::new(all_flairs_no_flairless.clone(), hot_data).name("Hot");
    let latest_data = collect_data(&all_flairs_no_flairless, latest_data_no_flairless);
    let latest_trace = Bar::new(all_flairs_no_flairless.clone(), latest_data).name("Latest");
    let top_data = collect_data(&all_flairs_no_flairless, top_data_no_flairless);
    let top_trace = Bar::new(all_flairs_no_flairless, top_data).name("Top");

    no_flairless_plot.add_trace(hot_trace);
    no_flairless_plot.add_trace(latest_trace);
    no_flairless_plot.add_trace(top_trace);

    no_flairless_plot.write_image("flairs_no_flairless.png", ImageFormat::PNG, 800, 600, 1.0);

    println!("--- Flair Post Data (No Flairless) ---");
    println!("Hot Flair Percentages:");
    print_percentages(&hot_data_flair);
    println!("\nLatest Flair Percentages:");
    print_percentages(&latest_data_flair);
    println!("\nTop Flair Percentages:");
    print_percentages(&top_data_flair);

    // Plot NSFW data
    let hot_data_nsfw = process_posts_nsfw(&hot);
    let latest_data_nsfw = process_posts_nsfw(&latest);
    let top_data_nsfw = process_posts_nsfw(&top);

    let mut nsfw_plot = Plot::new();
    nsfw_plot.add_trace(Bar::new(vec!["NSFW", "SFW"], hot_data_nsfw.to_vec()).name("Hot"));
    nsfw_plot.add_trace(Bar::new(vec!["NSFW", "SFW"], latest_data_nsfw.to_vec()).name("Latest"));
    nsfw_plot.add_trace(Bar::new(vec!["NSFW", "SFW"], top_data_nsfw.to_vec()).name("Top"));

    nsfw_plot.write_image("nsfw.png", ImageFormat::PNG, 800, 600, 1.0);

    println!("\n--- NSFW Post Data ---");
    println!("Hot NSFW Percentages:");
    print_percentages_nsfw(&hot_data_nsfw);
    println!("\nLatest NSFW Percentages:");
    print_percentages_nsfw(&latest_data_nsfw);
    println!("\nTop NSFW Percentages:");
    print_percentages_nsfw(&top_data_nsfw);
}
