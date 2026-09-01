package summarizer

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/aws/aws-sdk-go-v2/aws"
	awsconfig "github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/service/bedrockruntime"

	"claude-status/internal/transcript"
)

// bedrock returns the lazily-built Bedrock client. LoadDefaultConfig walks
// the full AWS credential chain (env, profile, SSO, IMDS), so it only runs
// when Bedrock auth is actually selected.
func (s *Summarizer) bedrock(ctx context.Context) (*bedrockruntime.Client, error) {
	s.brOnce.Do(func() {
		cfg, err := awsconfig.LoadDefaultConfig(ctx)
		if err != nil {
			s.brErr = err
			return
		}
		s.brClient = bedrockruntime.NewFromConfig(cfg)
	})
	return s.brClient, s.brErr
}

// callBedrock invokes the model through Bedrock. The body carries
// anthropic_version instead of a model field — the model id is the
// InvokeModel parameter — but the response uses the same Messages schema
// as the direct API, so parsing is shared.
func (s *Summarizer) callBedrock(ctx context.Context, client *bedrockruntime.Client, modelID string, snap *transcript.Snapshot) (string, error) {
	body, _ := json.Marshal(map[string]any{
		"anthropic_version": "bedrock-2023-05-31",
		"max_tokens":        60,
		"system":            claudeCodeIdent,
		"messages": []map[string]any{
			{"role": "user", "content": buildPrompt(snap)},
		},
	})
	out, err := client.InvokeModel(ctx, &bedrockruntime.InvokeModelInput{
		ModelId:     aws.String(modelID),
		ContentType: aws.String("application/json"),
		Accept:      aws.String("application/json"),
		Body:        body,
	})
	if err != nil {
		return "", fmt.Errorf("bedrock invoke %s: %w", modelID, err)
	}
	return parseMessagesResponse(out.Body)
}
