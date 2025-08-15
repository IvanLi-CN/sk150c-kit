// Simplified commitlint configuration without external dependencies

module.exports = {
  rules: {
    // Type rules - conventional commits types
    'type-enum': [
      2,
      'always',
      ['feat', 'fix', 'docs', 'style', 'refactor', 'perf', 'test', 'chore', 'ci', 'build', 'revert'],
    ],
    'type-case': [2, 'always', 'lower-case'],
    'type-empty': [2, 'never'],

    // Scope rules
    'scope-case': [2, 'always', 'lower-case'],

    // Subject rules
    'subject-empty': [2, 'never'],
    'subject-case': [2, 'always', 'lower-case'],
    'subject-full-stop': [2, 'never', '.'],
    'subject-max-length': [2, 'always', 50],

    // Header rules
    'header-max-length': [2, 'always', 72],

    // Body rules (optional)
    'body-leading-blank': [1, 'always'],
    'body-max-line-length': [2, 'always', 100],

    // Footer rules (optional)
    'footer-leading-blank': [1, 'always'],
    'footer-max-line-length': [2, 'always', 100],
  },

  // Custom plugin for English-only validation
  plugins: [
    {
      rules: {
        'english-only': (parsed) => {
          const { subject, body } = parsed;

          // Check subject for Chinese characters
          if (subject) {
            const chineseRegex = /[\u4e00-\u9fff]/;
            if (chineseRegex.test(subject)) {
              return [false, 'Subject must be in English only. Chinese characters are not allowed.'];
            }
          }

          // Check body for Chinese characters
          if (body) {
            const chineseRegex = /[\u4e00-\u9fff]/;
            if (chineseRegex.test(body)) {
              return [false, 'Body must be in English only. Chinese characters are not allowed.'];
            }
          }

          return [true];
        }
      }
    }
  ]
};
