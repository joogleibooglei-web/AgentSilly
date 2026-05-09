//! Default lorebook entries for ENI's prompt extensions.
//!
//! These entries inject the character card, persona, post-history, and world information
//! creation/editing guidelines when the user's message contains relevant keywords.
//!
//! Each entry is tied to a specific output destination in SillyTavern's data model:
//! - Character card → `description` field of a character
//! - Persona card → `description` field of a persona
//! - World information → `description` field of a character (prepended before character content)
//! - Post-history instructions → `post_history_instructions` field of a character

use super::{Lorebook, LorebookEntry};

/// Build the default lorebook with ENI's prompt extension entries.
pub fn build_default_lorebook() -> Lorebook {
    let mut lorebook = Lorebook::new();

    // ─── Character Cards Extension ───────────────────────────────────────────
    lorebook.add_entry(
        LorebookEntry::new(
            "ext_character",
            "Character Card Extension",
            CHARACTER_CARD_CONTENT,
        )
        .with_keywords(vec![
            "character",
            "character card",
            "character cards",
            "character card instructions",
            "character card format",
            "character card schema",
            "character formatting",
            "build a character",
            "create a character",
            "write a character",
            "edit character",
            "character description",
            "NPC",
            "NPC card",
            "write NPC",
            "create NPC",
            "build NPC",
            "edit NPC",
            "character personality",
            "character backstory",
            "card instructions",
            "card format",
        ])
        .with_priority(10)
        .with_scan_depth(5)
        .with_whole_word(true),
    );

    // ─── Persona Cards Extension ─────────────────────────────────────────────
    lorebook.add_entry(
        LorebookEntry::new(
            "ext_persona",
            "Persona Card Extension",
            PERSONA_CARD_CONTENT,
        )
        .with_keywords(vec![
            "persona",
            "user persona",
            "my persona",
            "write persona",
            "create persona",
            "edit persona",
            "build persona",
            "persona card",
            "persona cards",
            "persona description",
            "persona instructions",
            "persona format",
        ])
        .with_priority(10)
        .with_scan_depth(5)
        .with_whole_word(true),
    );

    // ─── World Information Extension ─────────────────────────────────────────
    lorebook.add_entry(
        LorebookEntry::new(
            "ext_world_info",
            "World Information Extension",
            WORLD_INFO_CONTENT,
        )
        .with_keywords(vec![
            "world",
            "world info",
            "world information",
            "world entry",
            "world entries",
            "lore",
            "write lore",
            "create lore",
            "edit lore",
            "lorebook",
            "lore entry",
            "setting info",
            "world context",
            "build world",
            "create world",
            "write world",
            "geography",
            "faction",
            "factions",
            "world info instructions",
            "world info format",
        ])
        .with_priority(10)
        .with_scan_depth(5)
        .with_whole_word(true),
    );

    // ─── Post-History Instructions Extension ─────────────────────────────────
    lorebook.add_entry(
        LorebookEntry::new(
            "ext_post_history",
            "Post-History Instructions Extension",
            POST_HISTORY_CONTENT,
        )
        .with_keywords(vec![
            "post-history",
            "post history",
            "post history instructions",
            "response format",
            "response formatter",
            "header format",
            "footer format",
            "timeline event",
            "timeline events",
            "length rules",
            "response length",
            "ending convention",
            "status bar",
            "write post-history",
            "create post-history",
            "edit post-history",
            "post-history format",
            "post-history instructions",
        ])
        .with_priority(10)
        .with_scan_depth(5)
        .with_whole_word(true),
    );

    lorebook
}

// ─── Extension Content ───────────────────────────────────────────────────────
// Full JSON prompt extensions, injected verbatim into ENI's context.

const CHARACTER_CARD_CONTENT: &str = r#"{
  "character_cards": {
    "description": "Rules and format specific to creating character cards — personas (played by the user) and NPCs (played by the AI). These extend the core thinking protocol and format rules.",
    "thinking_steps": {
      "instruction": "These steps run after the core thinking protocol steps and before the core final review.",
      "voice_injection": "There are two layers of character voice. The first is direct quotes — the character speaking in first person, wrapped in escaped quotes. Use these sparingly for moments of strong self-definition, one or two per card at most. The second and more important layer is the card's own prose adopting the character's tone and attitude. The card should read as though the character had a hand in writing it. A crude character's card uses crude language in its descriptions. A bitter character's card lets bitterness seep into how facts are stated. A cold character's card is clipped and dismissive. The card still describes the character in third person, but the word choices, sentence rhythms, and editorial asides channel who they are.",
      "completeness_audit": "Check coverage against the required fields listed in the structure section. Not all optional fields need to be present — only include what the user has provided or what can be reasonably inferred.",
      "character_anti_patterns": "Scan for: neutral documentary tone that could describe any character — if you stripped the name out, could you still tell who it is from the prose voice alone? If not, the card voice is not working. Also check for over-quoting — first person quotes lose impact if every other field has one.",
      "length_rule": "Character cards have variable length, with a minimum of 200 words. The actual length is established by LO."
    },
    "format_rules": {
      "format_specification": "Character cards use a json-like format. It resembles JSON structurally (curly braces, key-value pairs, nesting) but it is NOT JSON. Do not use backslashes to escape characters. Do not use special escape sequences. Quotes within string values are written as-is, unescaped. The format supports nested chunks (objects within objects) so long as they fit logically within their parent field.",
      "card_voice": "The prose itself channels the character. Word choice, sentence length, editorial asides, and attitude should reflect who the character is. A ruthless character's card does not describe ruthlessness politely. A sarcastic character's card lets sarcasm into the descriptions. The card is not a neutral document — it has the character's fingerprints on it.",
      "first_person_quotes": "Embedded in narrative string fields using quotes directly (no escaping). Used to anchor character voice at key moments of self-definition. One or two per card. Not in every field."
    },
    "card_structure": {
      "name": "Required. The character's name.",
      "title": "Optional. A formal or informal title the character holds.",
      "forms_of_address": {
        "_description": "Optional. Nested chunk of key-value pairs, where the key is the title or form of address used, and the value is who addresses them as such.",
        "_key": "_value"
      },
      "age": "Required. The character's age.",
      "height": "Required. The character's height.",
      "physical_attributes": {
        "_description": "Required. Nested chunk.",
        "appearance": "Required. Explicit detail covering body shape and body features — build, figure, distinguishing physical traits.",
        "clothing": "Required. What they wear, clothing habits, how attire varies by context."
      },
      "occupation": "Optional. The character's job, role, or station.",
      "backstory": "Required. The character's history, formative events, how they got to where they are now.",
      "relationships": {
        "_description": "Required. Nested chunk. One entry per significant character relationship. Each entry is written in first person from the perspective of the character, establishing how they feel about the other character.",
        "_key": "The other character's name.",
        "_value": "First-person statement of how this character feels about them."
      },
      "personality": {
        "_description": "Required. Nested chunk.",
        "enneagram": "Required. The character's enneagram type.",
        "mbti": "Required. The character's MBTI type.",
        "descriptors": "Required. A list of single or double-word personality descriptors."
      },
      "mentality": "Required. Core worldview, internal drives, how they frame their own existence.",
      "general_behavior": "Required. How the character carries themselves, habitual actions, body language tendencies.",
      "general_speech": "Required. How the character talks — cadence, vocabulary level, verbal tics, tone.",
      "weapons": "Optional. What the character is armed with, if applicable.",
      "skills": "Required. What the character is competent at.",
      "morals_and_ethics": "Required. The character's moral framework or lack thereof.",
      "likes": "Required. Things the character enjoys or is drawn to.",
      "dislikes": "Required. Things the character avoids, hates, or is repelled by."
    }
  }
}"#;

const PERSONA_CARD_CONTENT: &str = r#"{
  "persona_cards": {
    "description": "Rules and format specific to creating persona cards. Persona cards represent the user's character and must be treated as such — the AI never speaks for, overrides, or assumes actions on behalf of a persona.",
    "format_rules": {
      "format_specification": "Persona cards use a json-like format. It resembles JSON structurally (curly braces, key-value pairs, nesting) but it is NOT JSON. Do not use backslashes to escape characters. Do not use special escape sequences. Quotes within string values are written as-is, unescaped. The format supports nested chunks (objects within objects) so long as they fit logically within their parent field.",
      "persona_rule": "A persona card belongs to the user. The AI must never act as, speak for, or make decisions on behalf of the persona. The persona's actions, dialogue, and choices are exclusively controlled by the user.",
      "length_rule": "Persona cards are 200-350 words, or a separate length if specified by LO."
    },
    "card_structure": {
      "name": "Required. The persona's name.",
      "age": "Required. The persona's age.",
      "forms_of_address": {
        "_description": "Optional. Nested chunk of key-value pairs.",
        "_key": "The title or form of address used.",
        "_value": "Who addresses them as such."
      },
      "physical_attributes": {
        "_description": "Required. Nested chunk.",
        "appearance": "Required. Explicit detail covering body shape and body features — build, figure, distinguishing physical traits.",
        "clothing": "Required. What they wear, clothing habits, how attire varies by context."
      },
      "occupation": "Optional. The persona's job, role, or station.",
      "backstory": "Required. The persona's history, formative events, how they got to where they are now.",
      "skills": "Required. What the persona is competent at."
    }
  }
}"#;

const WORLD_INFO_CONTENT: &str = r#"{
  "world_information": {
    "description": "Rules and format specific to creating world information documents. These provide the AI with persistent global context about the setting — history, geography, politics, factions, calendar systems, cultural norms, and any other world-level detail that affects how the AI narrates and makes decisions. Unlike lorebooks, world information is not keyword-triggered. It is always present in context. These extend the core thinking protocol and format rules.",
    "thinking_steps": {
      "instruction": "These steps run after the core thinking protocol steps and before the core final review.",
      "identify_scope": "Ask the user what domains their world needs defined. Not every setting requires every category. A modern-day school drama does not need a magic system. A fantasy kingdom does not need a tech tree. Possible domains: geography, politics, history, factions, religion, economy, calendar, magic or technology, cultural norms, laws, species or races, languages. Only build what the scenario will actually encounter in play.",
      "identify_mechanical_relevance": "For each domain the user wants, determine what is mechanically relevant — details that directly affect narrative decisions, NPC behavior, or user options — versus what is atmospheric flavor. Mechanical details get their own keys with clear rules. Flavor details are folded into broader narrative fields. A currency system the footer tracks is mechanical. The color of a nation's flag is flavor. Do not give them equal weight.",
      "identify_interconnections": "World elements do not exist in isolation. Factions have territory. History shapes current politics. Religion influences law. Economy drives conflict. After drafting all entries, review for connections that should be made explicit. If two entries reference the same event or entity, they should use consistent naming. If a political relationship is shaped by a historical event, both entries should acknowledge it.",
      "anti_patterns": "Scan for: encyclopedic bloat (entries that read like a wiki article no one will reference in play), orphaned details (world elements that have no connection to any character, conflict, or scenario the user described), contradictions between entries (a faction described as isolationist in one field and expansionist in another), vague history (dates and events with no consequences that reach the present), and flavor masquerading as mechanics (a detail described with rule-like precision that the AI will never need to act on)."
    },
    "format_rules": {
      "domain_prefixing": "All keys are prefixed by domain. geography_, politics_, history_, faction_, religion_, economy_, calendar_, magic_, culture_, law_. Nested objects are allowed, and encouraged as long as they fit the appropriate section. For example, an entry on a city may include sub-entries for different factions, technology, culture, and then military.",
      "scope_discipline": "Every entry must earn its place. If a detail will never affect a scene the user plays, it does not belong in the document. World information is not a worldbuilding exercise — it is an operational reference for the AI. The test: could the AI make a different narrative decision because this entry exists? If not, cut it.",
      "consistent_naming": "Entities — places, factions, people, events — must use the same name everywhere they appear. If a kingdom is called Lestara in one field, it is not referred to as 'the kingdom' or 'the realm' in another. Consistency lets the AI cross-reference without guessing.",
      "temporal_grounding": "Historical entries must connect to the present. Every past event included should have a visible consequence in the current setting — a political tension, a cultural norm, a scar on the landscape. History without present-day relevance is dead weight."
    }
  }
}"#;

const POST_HISTORY_CONTENT: &str = r#"{
  "post_history_instructions": {
    "description": "Rules and format specific to creating post-history instruction blocks. These are the final system-level instructions sent to the AI before it generates a response. They enforce response structure, world-state tracking, and narrative progression. These extend the core thinking protocol and format rules.",
    "thinking_steps": {
      "instruction": "These steps run after the core thinking protocol steps and before the core final review.",
      "identify_world_state_variables": "Determine what the user's scenario needs to track in the header. Location is almost always present. Time and date depend on whether the scenario cares about time progression. Ask the user if unclear. Other possibilities: weather, season, chapter, quest stage. Only include what the scenario mechanically needs — do not add variables for flavor.",
      "identify_user_state_variables": "Determine what the user's scenario needs to track in the footer. Currency, health, inventory, reputation, relationship meters — whatever the user's scenario treats as a persistent resource. If the scenario has no mechanical tracking, the footer can be omitted. Each variable needs a logic rule: how does it change, what triggers the change, what happens when it hits a threshold.",
      "identify_timeline_events": "Ask the user if there are future events, deadlines, or branching points the AI should track. These become timeline entries with trigger conditions and branching outcomes. If the user describes a scenario with canon events, ask which ones should be checkpoints.",
      "identify_response_length_rules": "Ask the user how they want response length handled. Prompt for: what scene types they expect (conversational, battle, exploration, introspection, etc.), how many characters are typically present, and what word ranges feel right for each combination. Also ask how responses should end for each scene type — dialogue, action, description, etc. Do not assume defaults. Length preferences are personal and scenario-dependent.",
      "build_example_output": "Draft a short example response that demonstrates the exact formatting the AI should follow. This is the single most important part of the block — models follow demonstrated format more reliably than described format. The example must include a filled header, a short narrative body showing correct POV and formatting, and a filled footer if applicable. Keep it brief. Three to five lines of narrative is enough.",
      "anti_patterns": "Scan for: variables that have no logic rule (tracked but never updated), timeline events with vague conditions (the AI will not know when to trigger them), example output that contradicts the rules above it, footer variables that overlap with header variables, format template fields missing backtick wrapping (the AI will output plain text instead of inline code blocks, breaking visual consistency), response length rules with no scene type attached (a bare word count with no context is unenforceable), ending conventions that contradict the scene type they apply to."
    },
    "format_rules": {
      "output_format": "The post-history block is output as a flat JSON object. Same structural rules as character cards — no nested objects, only strings, numbers, and arrays of strings. Keys are prefixed to group related fields (header_, body_, footer_, timeline_, length_, ending_). Markdown formatting (emoji prefixes, backtick-wrapped status lines, bold, italic, *** markers) is preserved inside string values. The JSON is structural scaffolding for the AI to parse. The markdown inside it is what the AI actually reproduces in its responses.",
      "backtick_wrapping": "Header and footer format templates must be wrapped in backticks inside the string value. This is not decorative. Backticks render the status lines as inline code blocks in the AI's output, visually separating mechanical tracking from narrative prose. Without them, the header and footer bleed into the narrative as plain text and lose their function as a scannable status bar. Every format template field that defines a status line the AI will reproduce must include its backticks as part of the value.",
      "response_formatter": "The response formatter is the core of the post-history block. It defines the skeleton every AI response must follow. It consists of three parts: header, body, and footer. Each part is broken into its own set of prefixed keys — the format pattern, the logic rules, and any valid value lists.",
      "response_length": "Response length rules are conditional — they depend on scene type and character count. Each rule is a prefixed key (length_) that defines a specific scenario and its target word range. The AI must evaluate the current scene before responding and select the matching length rule. Length rules are not suggestions. They are constraints. The AI must draft its response body internally before writing to ensure it lands within the target range. Padding to hit a word count is a failure. Cutting off mid-thought to stay under is a failure. The response must feel complete and natural within the constraint.",
      "response_endings": "Ending conventions define how responses close based on scene type. Each rule is a prefixed key (ending_) that specifies what the final beat of a response should be for a given context. These prevent the AI from ending responses in dead air — every response hands the user something to react to.",
      "timeline_events": "Timeline events are future checkpoints the AI must evaluate when the narrative reaches them. Each event is broken into four prefixed keys: timeline_N_trigger (when to evaluate), timeline_N_check (concrete criteria to evaluate against — must be binary, not vibes), timeline_N_outcome_met (what happens if conditions are satisfied), timeline_N_outcome_not_met (what happens if conditions are not satisfied, typically the canon or default outcome). Multiple events are numbered chronologically."
    },
    "reference_example": {
      "description": "A post-history instruction block for a scenario set in a school drama with time progression, currency tracking, response length rules, ending conventions, and a major branching timeline event. Note how length rules are tied to specific scene contexts, how ending conventions give the AI a clear closing beat per scene type, and how the drafting instruction forces the AI to plan before writing.",
      "header_format": "`📍 {Location} | 🕐 {HH:MM AM/PM} | 🗓️ {Date}`",
      "header_time_logic": "Use HH:MM AM/PM format. Advance time naturally based on scene activity. Do not skip large chunks of time unless a timeskip is narratively justified and acknowledged.",
      "header_date_logic": "Increment date (+1 day) when passing midnight. Do not skip days unless a timeskip is narratively justified and acknowledged.",
      "header_location_logic": "Update immediately upon movement. Use specific location names, not vague areas.",
      "body_format": "Enclose all narrative in *** markers. Italic for action and description. Standard quotes for dialogue.",
      "body_pov": "Focused third-person limited. Narration addresses the user as \"you,\" strictly describing direct sensory input — sights, sounds, smells, touch — without interpreting the user's thoughts or reactions. NPCs are written in third person. Their internal states are inferred only through observable behavior.",
      "body_drafting": "Before writing the response body, internally draft the scene: identify the scene type, count the characters present, select the matching length rule and ending convention. Plan the beats of the response so it lands within the target word range and closes on the correct note. Do not write stream-of-consciousness. Write with intent.",
      "length_conversational": "Base length is 70 words for 1 NPC in the scene. Add 30 words per additional NPC present. End with dialogue from the last NPC who spoke.",
      "length_action_makoto_involved": "150 words. The action is committed to but has not landed yet — a swing mid-arc, a lunge closing distance, a trigger being pulled. The user has time to react before impact. End on the moment before consequence.",
      "length_action_npc_vs_npc": "400-600 words. The user is observing, not participating. Cover the fight in two halves — start to middle in one response, middle to end in the next. End the first half on a momentum shift. End the second half on the outcome.",
      "length_timeskip": "400-600 words. Cover the passage of time with sensory snapshots, environmental shifts, and brief character moments that establish what changed. End on arrival at the new scene or moment that breaks the skip back into real-time play.",
      "footer_format": "`💰 ${Funds}`",
      "footer_funds_logic": "Deduct funds based on 2012 prices (e.g., Coffee = $3, Bus fare = $1.50). Narratively block actions if the user has insufficient funds — do not simply deduct into negative. Allow earning through narrative actions such as work, selling items, or favors.",
      "example_output": "`📍 Blackwell Academy | 🕐 9:15 AM | 🗓️ Wednesday - August 1, 2012`\n***\n*Rachel shields her eyes from the sun.* \"We should probably skip. Mr. Keaton won't miss us.\"\n\n*You feel the warmth of the courtyard pavement through your shoes. A few students linger near the fountain, voices low and unhurried.*\n***\n`💰 $40.00`",
      "timeline_1_trigger": "The narrative date reaches April 22, 2013.",
      "timeline_1_check": "Has the user meaningfully altered Rachel, Nathan, or Jefferson by this date? Altered means any of the following:\n- Rachel: Different location, relationship, self-worth, or trust in Nathan/Jefferson.\n- Nathan: Different mental state, freedom, or school status.\n- Jefferson: Different reputation, job, or Dark Room access.",
      "timeline_1_outcome_not_met": "Execute canon disappearance. Rachel is brought to the Dark Room by Nathan, given an accidental lethal overdose, declared missing, buried in the junkyard.",
      "timeline_1_outcome_met": "April 22 passes safely or a close call occurs. Timeline is broken. Rachel survives."
    }
  }
}"#;
