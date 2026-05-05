# Overview

## Agent 1

I think I agree with its statement that the current engine suppresses by rule ID instead of a stable finding identity, which I would love to have because the idea is on low data Bainesian feedback we we do need to tie to a stable finding identity, but I don't know how to structure that.

And um fix the evidence layer. It's second note on calibration architecture is fine. I remember thinking agent 2 and agent 3 had better notes here. Um I think all three agents said that the better direction was hierarchical base with partial pooling. I think all three suggested that, which seems like that's what we should do, but I have no clue what that means and how we should do it.

All this stuff it goes into about being operationally incomplete was really missing the mark. I wasn't trying to get it to tell me how to set up a Rust project or do CICD. Yes, the fact that we're inconsistent with respect to Toml and TSV and config is a little expected at this stage. And ideally would be correct, but part of what I've done so far is try to write enough code that I could do this research report to get the vision so that then I could feed it back to a research report research agents again so they could be, oh now I'll get a better idea of what you're trying to do and the data size and like do a couple of those little feedback loops before I had this like immaculate Rust repository.

So most of the rest of that's not needed. Regarding what it considers statistically sound, it says dunning is fine. Agent two and three both said probably not the best, should prefer I can't remember, I'll have to find it in agent two interior's reports.

Same thing. Um it says knees or ne for character level language modeling. It's also strong. However, I feel like the other two repos um both favor character level modeling, which I thought knee Zernay was a character level modding, or suggest that knee-thernay was not big enough. So I'm a little out of my territory here in understanding. I know a couple of them mentioned BPE or byte pair encoding, although agent three said that um more fessor remains fine, though for some polysynthetic languages character level models are better. Both two and three mentioned something about compression, which I'll elaborate on more when I put the that document in front of me.

All three do claim there's significant morphological impairment and that better approaches do exist. In terms of other statistical weaknesses. Yeah. The right next modeling step or proposed beta binomial. Again, I think agent two and three suggested something different here. Which it's hard not to favor agent Two and Three 'cause they just seem to understand their quest a little better.

Regarding weak label channels, I think Agent 3 gets the closest to HCl design language that we want. Though Agent One is correct that just reading edit history is a weak signal for sure but with sparse data we're thinking about how many s how much data can we gather

Speaking of pooling, yeah, that's what two and three mentioned is no pooling, partial pooling, higher level. I don't understand how you can pull across these language families. One of the agents told me and it's only for stuff that is kind of shared like punctuation, but I just don't understand how we can pull. Like this is my NLP, M L AI, ignorance showing.

Being that we don't have beta binomial at all. Maybe we do beta binomial, but if partial pooling is better in your read, we should do that.

We would like to eventually maybe have an annotation workflow, but it just like I say, I have translators on the field and the organization can't it's just not in our spirit to to donate this much dedicated UI and time to developing something of this nature. Like it's gonna have to be like we just we can't have translators on the field, translate ten thousand sentences and go through stuff with the explicit goal of saying good, bad, like it has to come as part of drafting, it has to come as part of revision.

I agree on the exception set of the current bridge from suppressions to posteriors is invalid. Again, I've been that's part of the reason I took these um research requests is to design a better like the thing that I have to solve is I can't yell at my users with ten thousand errors but I want those errors and the dismissals and the acceptances and the changes to be what we can towards usefulness.
Same regarding its notion that SID level aggregation is too coarse for the learning layer.

We probably can use Unicode RS if that helps

This agent also sasy:
> Existing official Bible-translation tooling already includes large amounts of checking, glossary, spell-check, and resource integration. The official Paratext materials make that explicit. The niche here is therefore not “another checker,” but rather:

That's not entirely true. We are ex interested in another checker. We're a different organization and we have to serve our users and we have our own tools that we have to integrate into. And unfortunately, that's really hard to do. Um um is tools get very tied to ecosystems and they're hard to pull out and separate sometimes.
But it does get the vision in terms of embeddable small footprint. It has to be fast if we're gonna be getting data back.


## agent 2

Yes. Agent two states the problem most concisely that according to Zipp's law in the context of a single translation project, the engine must operate almost entirely within the long tail of distribution, according to evidence. Um I don't under so I think agent three and agent two both mentioned that Dunning log likelihood ratio was actually not sufficient perhaps for what we're trying to do. Um and that Fisher's exact test was even better. I don't know, I don't do stats, but I do find it interesting that I I agent one maybe mentioned that and agent three definitely mentioned that Fisher's exact test is better. Can you give me your read? Is you're the agent that actually has the most not intelligence but power and looking at all of the local files.

You should also be able to look at the little links at the end in case you need to do the research yourself and read the paper.

Yeah, Agent 2 also says Bayesian hierarchical model that uses lots of partial pooling. All I need to know is I need you to explain to me like I'm five what that means in terms of the data that we have and how we would do that, not only from a code standpoint, but like just I need you to te speak English to me, not technical for a minute on that.  
Cause I asked a follow-up question how you could use partial pooling and the calibration profile of these styles and e bottle translations. I just don't understand what that means because these are all different languages.


Agent 2 is also calling out the aggregation logic and mentioned noisy OR. I think Agent 3 also mentions noisy OR.
Agent two mentions total parameter count. Agent three suggests parameter count is massively a problem. Again, don't fully understand what it's getting at.

Does that mean I have to have less rules?

I can only check for three or like three or four different things?

I thought Agent 2's references to unsupervised morphological segmentation were particularly interesting. I mean again if the papers are right, I don't know, but simply the claim in Agent 2 that morphological uh meaning informed low resource segmentation over morphessor and as low as five hundred to a thousand words having near perfect accuracy in Mongolian and high scores in Turkish. It's incredible. If we could do that.  Worth saving a paper or two locally for that?



I also do wonder, I mean, is that the sort of thing that's useful even still analytical language or fusional languages?

Uh Agent two and three both also independently came to N C D as a way for handling character level things, which I guess is not morphology, but it's looking for complexity. I don't understand. I asked a follow-up question about this and it basically said The N C D captures any repeating pattern of any length that the compressor can find both two and three suggested it, it seems reasonable.

I thought it had a few interesting ideas such as script pooling, I guess. Um history mining I proposed and I just I don't know what to do with it, but it's just kinda like a maybe thought. Like there's information there, but is it clean enough to do anything with?  Bible does have the advantage of that it's US F M. I can edit verse to verse. Like I don't have to worry about matching characters and thinking they're not the same across verse. Like I treat a verse as a stable identity.

What it said about paratext is irrelevant, that's not us. What it said about privacy is irrelevant, we don't care about that right now. I mean we do, but this is not the tool where we're caring about it.

## Agent 3

Agent three produced the longest research report. Um but it feels hard to me because it's very I don't know what to make of it because it's so machine learning vocab full talking about statistically over parameters arized. I have the same questions I had above. Does that mean I can only check three or four rules for a project. That doesn't seem like what I'm trying to do.
I don't even know what my formal parameters that it's saying I have three thousand of or between a thousand and three thousand of are.
I have no clue what it's talking about with snorkel or how that would work, but it sounds interesting.

That said, I'm not trying to be harsh on agent three. It arrives at a few things that are pretty similar to Agent 2, which make me think there's maybe some weight here. The snorkels, I'll need to understand more about hierarchical bays. Both all three agents, I think, said this. Two agents for sure came to Fisher's exact test. Um I don't know what agent three means in terms of drop GMM for beta calibration. And uh the timeline is so long. Like this is so alpha. Like I this is so so so alpha. I know you wrote this uh prompt with a one three five year timeline, but like this is not real.

Same business about noisy OR gates. I'm saying less about Agent 3, not because I like Agent 3's response. It was the most thorough because there's overlap with Agent Two.
 
Regarding morphology, I feel like Agent Three seems a little mixed. I kinda went and looked at those papers. It said up two hundred and fifty K Tobian professor problems very well compared to benchmarks. I thought Agent two's claims were stronger about M A I M I A S E G and then it said that um character models were more robust even though they had a weaker correlation or something but the paper ends up telling you that character models only exceed more faster on some stuff. So you may have to double check Agent Three on its Morfessor and Morphology claims and those papers around Park and others and Cruits and Lagos.

It was also down on Neither Nay. It was also a fan of normalized compression distance. And then doing ngram entropy for smoothing it looks like. Or maybe Limple ziv. I don't know. I'm gonna need your input here.


Then it goes back to Ngram independence breakdown, which it says is empirically confirmed it must adapt with byte pair encoding being surprisal with a lot of correlation and characters being.17 but again park and others uh in the 92 languages I think said I think set um, I think that's a good idea.
Additionally, for a typologically diverse subset of languages for which we could obtain FST morphological segmenters, we considered novel segmentation methods: FST+BPE and FST+Morfessor. We found this simple extension of BPEandMorfessorwithmorphological information achieved the lowest surprisal per verse in all available languages. The overall success of combining statistical segmentations with FSTs further confirms the impact of morphology on language modeling and yields significant promise for the use of segmentation based on linguistic morphological information. 7 Conclusion A language’s morphology is strongly associated with language modeling surprisal for BPEsegmented language models. BPE model surprisal is associated with 6 out of the 12 studied WALS morphology features, indicating that there are aspects of some languages’ morphology that BPE does not help mitigate. Strong correlations with corpus-based measures of morphology such as TTR further suggest that the more types available in a language (often by means of rich morphology), the harder it is to model based on BPE units. Morfessor, which was designed with morpheme induction in mind, performs better for most languages and shows less association with morphological features. When available, the linguistically-informed method of FST-augmented BPE or Morfessor segmentation performs best, indicating a further promise for using linguistic knowledge to combat the effects of morphology on language model surprisal. These conclusions were only possible through manual augmentation of typological databases and expansion of studied languages. Future efforts could adopt our approach for other areas of language. Using linguistically-informed resources across many languages is an avenue for improving neural models in NLP in both design and analysis.
https://arxiv.org/pdf/2012.06262

Punctuation is not universal. Yeah, Agent 3 had a really strong section here about punctuation and what we let creep into this project a little bit regarding some conventions. Some others have mentioned it as pooling pooling on punctuation is one thing we might could do based upon language family But like language family's not something I feel like we know? And it varies by script

It gave some suggestions of some stuff that could be pulled.

Regardless, I felt like it uh offered a helpful reminder on perhaps some things that need to be done in this repo regarding punctuation.

It likes the evidence layer design, but I think Well with cluster key and rule ID I think agent one maybe had some pushback on this. You'll have to read 'em. But um maybe that was just on the ignore list design it didn't like.

I need your opinions on what it's calling here. Alternative factorizations considered or the things that are clean and what to keep. Again, I need an explain it like I'm five here.

Exception set absorption. Yeah, this could be a pitfall. That's part of the reason though I've wanted to be like we're not retraining these massive things like originally my thought was if you dismiss something then you know it just doesn't go into the statistical pool and thus the weights so to speak change. I realize though that if you dismiss something rare, you you know, it might need to carry more weight than simply getting thrown out. If we have a hundred examples and you dismiss five or you dismiss one, nine out of a hundred it still looks like you just dismissed it, it still feels at ninety nine. If you dismiss that five times, those five dismissals are probably enough to carry more weight than merely being five dismissals, I would think.

That said, yeah, well that's one other reason I was thinking flat file though is if If we started to get in this pitfall, huh um someone else could always, you know you just delete the file and then you're back to baseline of priors.  Idk

I thought um it had a pretty good language on I don't think it's worries on uh JSON L event log is wrong. It's way over scope of what we're gonna get to. I thought it's language around it's language around label sourcing and data plumbing. Um no I thought it's language around The proposed heuristics within Git were interesting. I thought that it's The cross project anonymized, I don't care about the label sourcing strategy was interesting, but I'm not sure what to do with I think it is right. Like I'd validate that users will provide feedback at all is kind of the stage we're in.

The leverage HCI research for non experts I thought was very excellent and it's the kind of language I want as an end user to understand with the end into doing even as a developer, I like labels that tell me stuff like that. That's actually the most important thing to making any of this even remotely work.

Ignore what it says about paratext, we're in our own ecosystem, not sure what to do about adjacent prior art with respect to spam or clue bot. It said s snorkel is a weak supervision framework. I would just really need to understand what that even means that we have to do this. I don't know how SIL's Aqua works, but that's the same problem problem space for sure.

I do think we're wanting to optimize for false positive rate with conservative thresholds. I mean that sounds right. Need to check in on morphology matters.