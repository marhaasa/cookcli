use anyhow::Result;
use clap::Args;
use cooklang_import::fetch_recipe;
use tracing::{info, warn};
// use anthropic::{client::ClientBuilder, types::CompleteRequestBuilder, HUMAN_PROMPT, AI_PROMPT};

use crate::Context;

#[derive(Debug, Args)]
pub struct ImportArgs {
    /// URL of the recipe to import
    url: String,

    /// Skip conversion to Cooklang format and just fetch the original recipe
    #[arg(short, long)]
    skip_conversion: bool,

    /// Use Claude API instead of OpenAI for recipe conversion
    #[arg(long)]
    use_claude: bool,
}

pub fn run(_ctx: &Context, args: ImportArgs) -> Result<()> {
    
    let recipe = tokio::runtime::Runtime::new()?.block_on(async {
        if args.skip_conversion {
            info!("Fetching recipe without conversion from: {}", args.url);
            let recipe = fetch_recipe(&args.url)
                .await
                .map_err(|e| {
                    warn!("Fetch failed: {}", e);
                    anyhow::anyhow!("Failed to fetch recipe: {}", e)
                })?;
            info!("Successfully fetched recipe: {}", recipe.name);
            Ok(format!(
                "{}\n\n[Ingredients]\n{}\n\n[Instructions]\n{}",
                recipe.name, recipe.ingredients, recipe.instructions
            ))
        } else if args.use_claude {
            info!("Importing recipe with Claude conversion from: {}", args.url);
            
            // First try to fetch the recipe to see if that works
            info!("Step 1: Fetching recipe data...");
            let recipe_data = fetch_recipe(&args.url)
                .await
                .map_err(|e| {
                    warn!("Recipe fetch failed: {}", e);
                    anyhow::anyhow!("Failed to fetch recipe data: {}", e)
                })?;
            
            info!("Step 1 successful. Recipe name: {}", recipe_data.name);
            info!("Ingredients length: {}", recipe_data.ingredients.len());
            info!("Instructions length: {}", recipe_data.instructions.len());
            
            // Now try the conversion with Claude
            info!("Step 2: Converting recipe with Claude...");
            
            let anthropic_api_key = std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY must be set in the environment"))?;
            
            let client = reqwest::Client::new();
            
            let prompt = format!(
                "As a distinguished Cooklang Converter, your primary task is
    to transform recipes provided by the user into the structured
    Cooklang recipe markup format.

    Ingredients

    To define an ingredient, use the @ symbol. If the ingredient's
    name contains multiple words, indicate the end of the name with {{}}.

    Example:
        Then add @salt and @ground black pepper{{}} to taste.

    To indicate the quantity of an item, place the quantity inside {{}} after the name.

    Example:
        Poke holes in @potato{{2}}.

    To use a unit of an item, such as weight or volume, add a % between
    the quantity and unit.

    Example:
        Place @bacon strips{{1%kg}} on a baking sheet and glaze with @syrup{{1/2%tbsp}}.
    
    Many recipes involve repetitive ingredient preparations, such as peeling or chopping. To simplify this, you can define these common preparations directly within the ingredient reference using shorthand syntax:
    
    Example:
        Mix @onion{{1}}(peeled and finely chopped) and @garlic{{2%cloves}}(peeled and minced) into paste.

    Cookware

    You can define any necessary cookware with # symbol. If the cookware's
    name contains multiple words, indicate the end of the name with {{}}. For cookware it is especially important that you only use # the first time it is mentioned or else cooklang will create a cookware list with repeated items.

    Example:
        Place the potatoes into a #pot.
        Mash the potatoes with a #potato masher{{}}.

    Timer

    You can define a timer using ~.

    Example:
        Lay the potatoes on a #baking sheet{{}} and place into the #oven{{}}. Bake for ~{{25%minutes}}.

    Timers can have a name too.

    Example:
        Boil @eggs{{2}} for ~eggs{{3%minutes}}.

    User will give you a classical recipe representation when ingredients listed first
    and then method text.

    Final result shouldn't have original ingredient list, you need to
    incorporate each ingredient and quantities into method's text following
    Cooklang conventions.

    Ensure the original recipe's words are preserved, modifying only
    ingredients and cookware according to Cooklang syntax. Don't convert
    temperature.

    Separate each step with two new lines.

    Recipe Name: {}

    Ingredients:
    {}

    Instructions:
    {}",
                recipe_data.name,
                recipe_data.ingredients,
                recipe_data.instructions
            );
            
            let claude_response = client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", anthropic_api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&serde_json::json!({
                    "model": "claude-sonnet-4-20250514",
                    "max_tokens": 1000,
                    "messages": [
                        {
                            "role": "user",
                            "content": prompt
                        }
                    ]
                }))
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Claude API request failed: {}", e))?;
            
            let status = claude_response.status();
            if !status.is_success() {
                let error_text = claude_response.text().await
                    .unwrap_or_else(|_| "Failed to get error response".to_string());
                return Err(anyhow::anyhow!("Claude API failed with status {}: {}", status, error_text));
            }
            
            let claude_json: serde_json::Value = claude_response.json()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to parse Claude response: {}", e))?;
            
            let converted_recipe = claude_json["content"][0]["text"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Failed to extract content from Claude response"))?
                .to_string();
            
            info!("Claude conversion successful");
            Ok(converted_recipe)
        } else {
            info!("Importing recipe with OpenAI conversion from: {}", args.url);
            info!("OPENAI_API_KEY is set: {}", std::env::var("OPENAI_API_KEY").is_ok());
            
            // First try to fetch the recipe to see if that works
            info!("Step 1: Fetching recipe data...");
            let recipe_data = fetch_recipe(&args.url)
                .await
                .map_err(|e| {
                    warn!("Recipe fetch failed: {}", e);
                    anyhow::anyhow!("Failed to fetch recipe data: {}", e)
                })?;
            
            info!("Step 1 successful. Recipe name: {}", recipe_data.name);
            info!("Ingredients length: {}", recipe_data.ingredients.len());
            info!("Instructions length: {}", recipe_data.instructions.len());
            
            // Now try the full import with conversion
            info!("Step 2: Converting recipe with OpenAI...");
            
            // Let's test OpenAI directly with a simple request
            info!("Testing OpenAI API directly first...");
            let openai_api_key = std::env::var("OPENAI_API_KEY").unwrap();
            let client = reqwest::Client::new();
            let test_response = client
                .post("https://api.openai.com/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", openai_api_key))
                .json(&serde_json::json!({
                    "model": "gpt-4",
                    "messages": [
                        {"role": "user", "content": "Say hello"}
                    ],
                    "max_tokens": 10
                }))
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("OpenAI API request failed: {}", e))?;
            
            let status = test_response.status();
            let response_text = test_response.text().await.unwrap_or_else(|_| "Failed to get response text".to_string());
            
            info!("OpenAI API test response status: {}", status);
            info!("OpenAI API test response body: {}", response_text);
            
            if !status.is_success() {
                return Err(anyhow::anyhow!("OpenAI API test failed with status {}: {}", status, response_text));
            }
            
            // Now try the full import
            info!("OpenAI API test successful, trying full import...");
            // Note: Using fetch + manual conversion since import_recipe from cooklang-import may not work
            let prompt = format!(
                "Convert this recipe to Cooklang format. Cooklang is a markup language for recipes that uses @ingredient{{amount}} for ingredients, #cookware for cookware, and ~time{{minutes}} for timers.

Recipe Name: {}

Ingredients:
{}

Instructions:
{}

Please convert this to proper Cooklang format with ingredients marked as @ingredient{{amount}}, cookware as #cookware, and timers as ~timer{{time}}. Return only the converted recipe.",
                recipe_data.name,
                recipe_data.ingredients,
                recipe_data.instructions
            );
            
            let openai_response = client
                .post("https://api.openai.com/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", openai_api_key))
                .json(&serde_json::json!({
                    "model": std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-3.5-turbo".to_string()),
                    "messages": [
                        {"role": "user", "content": prompt}
                    ],
                    "max_tokens": 1000
                }))
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("OpenAI API request failed: {}", e))?;
            
            let openai_json: serde_json::Value = openai_response.json()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to parse OpenAI response: {}", e))?;
            
            let converted_recipe = openai_json["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Failed to extract content from OpenAI response"))?;
            
            info!("OpenAI conversion successful");
            Ok(converted_recipe.to_string())
        }
    })?;

    println!("{}", recipe);
    Ok(())
}
